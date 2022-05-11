import { DataReader } from './dataReader'
import { MapGroupObj, MapLayer, MapLayerQuads, MapLayerTiles, LayerQuad, LayerTile, MapImage, Color, Coord } from './types'

export function parseMapGroup(groupData: ArrayBuffer): MapGroupObj {
	let d = new DataReader(groupData)
	d.reset()

  return {
  	version: d.uint32(),
  	offX: d.int32(),
  	offY: d.int32(),
  	parallaxX: d.int32(),
  	parallaxY: d.int32(),
  	startLayer: d.uint32(),
  	numLayers: d.uint32(),
  	useClipping: d.uint32(),
		
		// version 2 extension
  	clipX: d.int32(),
  	clipY: d.int32(),
  	clipW: d.int32(),
  	clipH: d.int32(),
		
		// version 3 extension
		name: d.int32Str(3),
  }
}

export function parseMapLayer(layerData: ArrayBuffer): MapLayer {
	let d = new DataReader(layerData)
	d.reset()

  return {
  	version: d.uint32(),
  	type: d.uint32(),
  	flags: d.uint32(),
  }
}

export function parseMapLayerQuads(layerData: ArrayBuffer): MapLayerQuads {
	let d = new DataReader(layerData)
	d.reset()

	/*obj.version =*/ d.uint32()
	/*obj.type =*/ d.uint32()
	/*obj.flags =*/ d.uint32()

  return {
  	version: d.uint32(),
  	numQuads: d.uint32(),
  	data: d.int32(),
  	image: d.int32(),
		name: d.int32Str(3),
  }
}

export function parseMapLayerTiles(layerData: ArrayBuffer): MapLayerTiles {
	let d = new DataReader(layerData)
	d.reset()

	/*obj.version =*/ d.uint32()
	/*obj.type =*/ d.uint32()
	/*obj.flags =*/ d.uint32()

  return {
  	version: d.uint32(),
  	width: d.int32(),
  	height: d.int32(),
  	flags: d.uint32(),
	
  	color: {
      r: d.uint32() & 0xff,
      g: d.uint32() & 0xff,
      b: d.uint32() & 0xff,
      a: d.uint32() & 0xff,
    },

  	colorEnv: d.int32(),
  	colorEnvOffset: d.int32(),

  	image: d.int32(),
  	data: d.int32(),

		name: d.int32Str(3),
  }

	//TODO: name
}

export function parseMapImage(layerData: ArrayBuffer): MapImage {
	let d = new DataReader(layerData)
	d.reset()
  
  return {
  	version: d.uint32(),
  	width: d.int32(),
  	height: d.int32(),
  	external: d.uint32(),
  	name: d.int32(),
  	data: d.int32(),
  }
}

export function parseLayerQuads(layerData: ArrayBuffer, num: number): LayerQuad[] {
	let quads: LayerQuad[] = []

	let d = new DataReader(layerData)
	d.reset()

	for (let q = 0; q < num; q++) {

		let points = []
		for (let i = 0; i < 5; i++) {
			points.push({
        x: d.int32(),
        y: d.int32(),
      })
		}

		let colors: Color[] = []
		for (let i = 0; i < 4; i++) {
			colors.push({
				r: d.uint32()&0xff,
				g: d.uint32()&0xff,
				b: d.uint32()&0xff,
				a: d.uint32()&0xff
			})
		}

		let texCoords: Coord[] = []
		for (let i = 0; i < 4; i++) {
			texCoords.push({x: d.int32(), y: d.int32()})
		}
    
    let quad = {
      points,
      colors,
      texCoords,
      posEnv: d.int32(),
      posEnvOffset: d.int32(),
      colorEnv: d.int32(),
      colorEnvOffset: d.int32(),
    }

    quads.push(quad)
	}

	return quads
}

export function parseLayerTiles(tileData: ArrayBuffer, num: number): LayerTile[] {
	let tiles: LayerTile[] = []
	let d = new DataReader(tileData)
	d.reset()

	for (let i = 0; i < num; i++) {
		tiles.push({
      index: d.uint8(),
      flags: d.uint8(),
    })
		
		// skip reserved
		d.uint8()
    d.uint8()
	}

	return tiles
}

export function parseString(data: ArrayBuffer) {
  let buf = new Uint8Array(data)
  
  let len = 0
  
  for(; len < buf.byteLength; len++)
    if (buf[len] === 0)
      break
  
  return String.fromCharCode.apply(null, buf.subarray(0, len))
}