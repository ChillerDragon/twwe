import { Server } from './server/server'
import { ChangeData, MapInfo } from './server/protocol'
import { Map } from './twmap/map'
import { RenderMap } from './gl/renderMap'
import { init as glInit, renderer, viewport } from './gl/global'
import { TreeView } from './ui/treeView'
import { TileSelector } from './ui/tileSelector'
import { LayerType } from './twmap/types'


// all html elements are prefixed with $, but no JQuery :)
let $canvas: HTMLCanvasElement = document.querySelector('canvas')
let $nav: HTMLElement = document.querySelector('nav')
let $tree: HTMLElement = $nav.querySelector('#tree')
let $selector: HTMLElement = document.querySelector('#tile-selector')
let $mapName: HTMLElement = $nav.querySelector('#map-name')
let $dialog: HTMLElement = document.querySelector('#dialog')
let $dialogContent: HTMLElement = $dialog.querySelector('.content')
let $users: HTMLElement = document.querySelector('#users span')
let $btnSave: HTMLElement = document.querySelector('#save')
let $btnToggleNav: HTMLElement = document.querySelector('#nav-toggle')
let $lobby: HTMLElement = document.querySelector('#lobby')
let $lobbyContent: HTMLElement = $lobby.querySelector('.content')

let map: Map
let rmap: RenderMap
let server: Server

let treeView: TreeView
let tileSelector: TileSelector


function showDialog(msg: string) {
  $dialogContent.innerText = msg
  $dialog.classList.remove('hidden')
}

function hideDialog() {
  $dialogContent.innerText = ''
  $dialog.classList.add('hidden')
}

async function setupServer() {
  // setup server  
  server = await Server.create('127.0.0.1', 16900)
  .catch((e) => {
    showDialog('Failed to connect to the server.')
    throw e
  })
  
  server.on('change', (e) => {
    rmap.applyChange(e)
  })

  server.on('users', (e) => {
    $users.innerText = e.count + ''
  })
  
  $btnSave.addEventListener('click', () => {
    server.send('save')
  })
}

function setupGL() {

  glInit($canvas)
  rmap = new RenderMap(map)

  function loop() {
    renderer.render(rmap)
    requestAnimationFrame(loop)
  }
  requestAnimationFrame(loop)
}

function placeTile() {
    let { x, y } = viewport.mousePos
    x = Math.floor(x)
    y = Math.floor(y)

    let [ group, layer ] = treeView.getSelected()
    let id = tileSelector.getSelected()
    
    let change: ChangeData = {
      group, 
      layer,
      x,
      y,
      id,
    }
  
    let res = rmap.applyChange(change)

    // only apply change if succeeded e.g. not redundant 
    if(res) {
      console.log('change:', change)
      server.send('change', change)
    }
}

function setupUI() {
  treeView = new TreeView($tree, map)
  tileSelector = new TileSelector($selector)

  let [ groupID, layerID ] = map.gameLayerID()
  
  treeView.onselect = (groupID, layerID) => {
    let layer = map.groups[groupID].layers[layerID]
    let image = layer.image
    if (layer.type === LayerType.GAME)
      image = rmap.gameLayer.texture.image
    tileSelector.setImage(image)
  }

  treeView.select(groupID, layerID)

  $mapName.innerText = map.name

  viewport.onclick = () => placeTile()
  
  window.addEventListener('keydown', (e) => {
    e.preventDefault()
    if (e.key === ' ')
      placeTile()
  })
  
  $btnToggleNav.addEventListener('click', () => {
    $nav.classList.toggle('hidden')
  })
}

function chooseMap(mapInfos: MapInfo[]): Promise<string> {
  return new Promise(resolve => {
    $lobbyContent.innerHTML = ''

    for (const info of mapInfos) {
      const $btn = document.createElement('button')
      $btn.innerText = 'Join'
      $btn.onclick = () => {
        $lobby.classList.add('hidden')
        $lobbyContent.innerHTML = ''
        resolve(info.name)
      }

      const $name = document.createElement('span')
      $name.classList.add('name')
      $name.innerText = info.name

      const $users = document.createElement('span')
      $users.classList.add('users')
      $users.innerText = '' + info.users

      $lobbyContent.append($name, $users, $btn)
    }

    $lobby.classList.remove('hidden')
  })
}

async function main() {
  
  try {
    showDialog('Connecting to server…')
    await setupServer()
    let mapInfos = await server.query('maps')
    hideDialog()
    let mapName = await chooseMap(mapInfos)
    showDialog('Joining room…')
    let joined = await server.query('join', mapName)
    if (!joined) throw 'failed to join the room'
    let buf = await server.query('map')
    map = new Map(mapName, buf)
  }
  catch (e) {
    console.error(e)
    showDialog(e)
    return
  }
  
  setupGL()
  setupUI()
  hideDialog()
  console.log('up and running!')
}


main()