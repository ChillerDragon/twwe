extern crate pretty_env_logger;

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};

use futures::channel::mpsc::UnboundedSender;

use tungstenite::protocol::Message;

use twmap::{GameLayer, Group, Layer, LayerKind, QuadsLayer, TileMapLayer, TilesLayer, TwMap};

use crate::{
    protocol::*,
    twmap_map_edit::{extend_layer, shrink_layer},
    Peer,
};

type Tx = UnboundedSender<Message>;

fn server_error<E: std::fmt::Display>(err: E) -> &'static str {
    log::error!("{}", err);
    "internal server error"
}

fn load_map(path: &Path) -> Result<TwMap, twmap::Error> {
    let mut map = TwMap::parse_file(&path)?;
    map.load()?;
    Ok(map)
}

pub fn set_layer_width<T: TileMapLayer>(layer: &mut T, width: usize) -> Result<(), &'static str> {
    let old_width = layer.tiles().shape().0 as isize;
    let diff = width as isize - old_width;

    if width == 0 || width > 10000 {
        return Err("invalid layer dimensions");
    }

    if diff > 0 {
        extend_layer(layer, 0, 0, 0, diff as usize);
    } else if diff < 0 {
        shrink_layer(layer, 0, 0, 0, -diff as usize);
    }

    Ok(())
}

pub fn set_layer_height<T: TileMapLayer>(layer: &mut T, height: usize) -> Result<(), &'static str> {
    let old_height = layer.tiles().shape().0 as isize;
    let diff = height as isize - old_height;

    if height == 0 || height > 10000 {
        return Err("invalid layer dimensions");
    }

    if diff > 0 {
        extend_layer(layer, 0, diff as usize, 0, 0);
    } else if diff < 0 {
        shrink_layer(layer, 0, -diff as usize, 0, 0);
    }

    Ok(())
}

// We want the room to have the map loaded when at least 1 peer is connected, but unloaded
// when the last peer disconnects. The LazyMap provides these capabilities.
pub struct LazyMap {
    pub path: PathBuf,
    map: Arc<Mutex<Option<TwMap>>>,
}

impl LazyMap {
    fn new(path: PathBuf) -> Self {
        LazyMap {
            path,
            map: Arc::new(Mutex::new(None)),
        }
    }

    fn unload(&self) {
        *self.map.lock() = None;
        log::debug!("unloaded map {}", self.path.display());
    }

    pub fn get(&self) -> MappedMutexGuard<TwMap> {
        // lazy-load map if not loaded
        let mut map = self.map.lock();
        if *map == None {
            *map = load_map(&self.path).ok();
            log::debug!("loaded map {}", self.path.display());
        }
        match *map {
            Some(_) => MutexGuard::map(map, |m| m.as_mut().unwrap()),
            None => panic!("failed to load map {}", self.path.display()),
        }
    }
}

pub struct Room {
    peers: Mutex<HashMap<SocketAddr, Tx>>,
    pub map: LazyMap,
    saving: Mutex<()>, // this mutex prevents multiple users from saving at the same time
}

impl Room {
    pub fn new(path: PathBuf) -> Self {
        Room {
            peers: Mutex::new(HashMap::new()),
            map: LazyMap::new(path),
            saving: Mutex::new(()),
        }
    }

    pub fn add_peer(&self, peer: &Peer) {
        self.peers().insert(peer.addr, peer.tx.clone());
    }

    pub fn peer_count(&self) -> usize {
        self.peers().len()
    }

    pub fn remove_peer(&self, peer: &Peer) {
        self.peers().remove(&peer.addr);
        if self.peers().is_empty() {
            self.map.unload()
        }
    }

    pub fn peers(&self) -> MutexGuard<HashMap<SocketAddr, Tx>> {
        self.peers.lock()
    }

    pub fn send_map(&self, peer: &Peer) -> Result<(), &'static str> {
        let buf = {
            let mut buf = Vec::new();
            self.map.get().save(&mut buf).map_err(server_error)?; // TODO: this is blocking for the server
            buf
        };

        peer.tx.unbounded_send(Message::Binary(buf)).ok();

        Ok(())
    }

    pub fn save_copy(&self, path: &PathBuf) -> Result<(), &'static str> {
        // clone the map to release the lock as soon as possible
        self.map
            .get()
            .clone()
            .save_file(path)
            .map_err(server_error)?;

        log::debug!("cloned {} to {}", self.map.path.display(), path.display());
        Ok(())
    }

    pub fn save_map(&self) -> Result<(), &'static str> {
        // Avoid concurrent saves
        let _lck = self.saving.lock();

        // clone the map to release the lock as soon as possible
        self.map
            .get()
            .clone()
            .save_file(&self.map.path)
            .map_err(server_error)?;

        log::debug!("saved {}", self.map.path.display());
        Ok(())
    }

    pub fn set_tile(&self, edit_tile: &EditTile) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let layer = map
            .groups
            .iter_mut()
            .find_map(|g| {
                g.layers.iter_mut().find_map(|l| match l {
                    Layer::Game(gl) => Some(gl),
                    _ => None,
                })
            })
            .ok_or("no game layer")?;
        let tiles = layer.tiles.unwrap_mut(); // map must be loaded
        let mut tile = tiles
            .get_mut((edit_tile.y as usize, edit_tile.x as usize))
            .ok_or("tile change outside layer")?;
        tile.id = edit_tile.id;

        log::debug!("changed tile {:?}", edit_tile);
        Ok(())
    }

    pub fn create_group(&self, create_group: &CreateGroup) {
        let mut map = self.map.get();
        let mut group = Group::default();
        group.name = create_group.name.to_owned();
        map.groups.push(group);
    }

    pub fn edit_group(&self, edit_group: &EditGroup) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let mut group = map
            .groups
            .get_mut(edit_group.group as usize)
            .ok_or("invalid group index")?;

        use OneGroupChange::*;
        match edit_group.change.clone() {
            OffX(off_x) => group.offset_x = off_x,
            OffY(off_y) => group.offset_y = off_y,
            ParaX(para_x) => group.parallax_x = para_x,
            ParaY(para_y) => group.parallax_y = para_y,
            Name(name) => {
                if group.is_physics_group() {
                    return Err("cannot delete the physics group");
                }
                group.name = name
            }
        }

        Ok(())
    }

    pub fn reorder_group(&self, reorder_group: &ReorderGroup) -> Result<(), &'static str> {
        let mut map = self.map.get();

        if reorder_group.group as usize >= map.groups.len() {
            return Err("invalid group index");
        }

        let group = map.groups.remove(reorder_group.group as usize);

        if reorder_group.new_group as usize > map.groups.len() {
            return Err("invalid new group index");
        }

        map.groups.insert(reorder_group.new_group as usize, group);

        Ok(())
    }

    pub fn delete_group(&self, delete_group: &DeleteGroup) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let group = map
            .groups
            .get_mut(delete_group.group as usize)
            .ok_or("invalid group index")?;

        if group.is_physics_group() {
            return Err("cannot delete the physics group");
        }

        map.groups.remove(delete_group.group as usize);

        Ok(())
    }

    pub fn create_layer(&self, create_layer: &CreateLayer) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let default_layer_size = map.find_physics_layer::<GameLayer>().unwrap().tiles.shape();
        let group = map
            .groups
            .get_mut(create_layer.group as usize)
            .ok_or("invalid group index")?;
        match create_layer.kind {
            // LayerKind::Game => todo!(),
            LayerKind::Tiles => {
                let mut layer = TilesLayer::new(default_layer_size);
                layer.name = create_layer.name.to_owned();
                group.layers.push(Layer::Tiles(layer));
            }
            LayerKind::Quads => {
                let mut layer = QuadsLayer::default();
                layer.name = create_layer.name.to_owned();
                group.layers.push(Layer::Quads(layer));
            }
            // LayerKind::Front => todo!(),
            // LayerKind::Tele => todo!(),
            // LayerKind::Speedup => todo!(),
            // LayerKind::Switch => todo!(),
            // LayerKind::Tune => todo!(),
            // LayerKind::Sounds => todo!(),
            _ => return Err("invalid new layer kind"),
        }

        Ok(())
    }

    pub fn edit_layer(&self, edit_layer: &EditLayer) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let group = map
            .groups
            .get_mut(edit_layer.group as usize)
            .ok_or("invalid group index")?;
        let layer = group
            .layers
            .get_mut(edit_layer.layer as usize)
            .ok_or("invalid layer index")?;

        use OneLayerChange::*;
        match edit_layer.change.clone() {
            Name(name) => *layer.name_mut().ok_or("cannot change layer name")? = name,
            Color(color) => match layer {
                Layer::Tiles(layer) => layer.color = color,
                _ => return Err("cannot change layer color"),
            },
            Width(width) => match layer {
                Layer::Game(_)
                | Layer::Front(_)
                | Layer::Tele(_)
                | Layer::Speedup(_)
                | Layer::Switch(_)
                | Layer::Tune(_) => {
                    for layer in group.layers.iter_mut() {
                        match layer {
                            Layer::Game(layer) => set_layer_width(layer, width as usize)?,
                            Layer::Front(layer) => set_layer_width(layer, width as usize)?,
                            Layer::Tele(layer) => set_layer_width(layer, width as usize)?,
                            Layer::Speedup(layer) => set_layer_width(layer, width as usize)?,
                            Layer::Switch(layer) => set_layer_width(layer, width as usize)?,
                            Layer::Tune(layer) => set_layer_width(layer, width as usize)?,
                            Layer::Tiles(_)
                            | Layer::Quads(_)
                            | Layer::Sounds(_)
                            | Layer::Invalid(_) => (),
                        }
                    }
                }
                Layer::Tiles(layer) => set_layer_width(layer, width as usize)?,
                Layer::Quads(_) | Layer::Invalid(_) | Layer::Sounds(_) => {
                    return Err("cannot change layer dimensions")
                }
            },
            Height(height) => match layer {
                Layer::Game(_)
                | Layer::Front(_)
                | Layer::Tele(_)
                | Layer::Speedup(_)
                | Layer::Switch(_)
                | Layer::Tune(_) => {
                    for layer in group.layers.iter_mut() {
                        match layer {
                            Layer::Game(layer) => set_layer_height(layer, height as usize)?,
                            Layer::Front(layer) => set_layer_height(layer, height as usize)?,
                            Layer::Tele(layer) => set_layer_height(layer, height as usize)?,
                            Layer::Speedup(layer) => set_layer_height(layer, height as usize)?,
                            Layer::Switch(layer) => set_layer_height(layer, height as usize)?,
                            Layer::Tune(layer) => set_layer_height(layer, height as usize)?,
                            Layer::Tiles(_)
                            | Layer::Quads(_)
                            | Layer::Sounds(_)
                            | Layer::Invalid(_) => (),
                        }
                    }
                }
                Layer::Tiles(layer) => set_layer_height(layer, height as usize)?,
                Layer::Quads(_) | Layer::Invalid(_) | Layer::Sounds(_) => {
                    return Err("cannot change layer dimensions")
                }
            },
        }

        Ok(())
    }

    pub fn reorder_layer(&self, reorder_layer: &ReorderLayer) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let group = map
            .groups
            .get_mut(reorder_layer.group as usize)
            .ok_or("invalid group index")?;

        if reorder_layer.layer as usize >= group.layers.len() {
            return Err("invalid layer index");
        }

        if reorder_layer.group != reorder_layer.new_group {
            let layer = group.layers.remove(reorder_layer.layer as usize);
            let new_group = map
                .groups
                .get_mut(reorder_layer.new_group as usize)
                .ok_or("invalid new group index")?;

            if reorder_layer.new_layer as usize > new_group.layers.len() {
                return Err("invalid new layer index");
            }

            if layer.kind() == LayerKind::Game {
                return Err("cannot reorder the game layer");
            }

            new_group
                .layers
                .insert(reorder_layer.new_layer as usize, layer);
        } else if reorder_layer.layer != reorder_layer.new_layer {
            let layer = group.layers.remove(reorder_layer.layer as usize);

            if reorder_layer.new_layer as usize > group.layers.len() {
                return Err("invalid new layer index");
            }

            group.layers.insert(reorder_layer.new_layer as usize, layer);
        }

        Ok(())
    }

    pub fn delete_layer(&self, delete_layer: &DeleteLayer) -> Result<(), &'static str> {
        let mut map = self.map.get();
        let group = map
            .groups
            .get_mut(delete_layer.group as usize)
            .ok_or("invalid group index")?;
        let layer = group
            .layers
            .get_mut(delete_layer.layer as usize)
            .ok_or("invalid layer index")?;

        if layer.kind() != LayerKind::Game {
            group.layers.remove(delete_layer.layer as usize);
            Ok(())
        } else {
            Err("cannot delete the game layer")
        }
    }
}
