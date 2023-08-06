import type * as Info from '../twmap/types'
import type * as MapDir from '../twmap/mapdir'
import type { CurveTypeStr, EditTile, EditTileParams } from './protocol'
import * as Parser from '../twmap/parser'

const curveTypeStr: CurveTypeStr[] = ['step', 'linear', 'slow', 'fast', 'smooth', 'bezier']

export function curveTypeToString(curve: Info.CurveType): CurveTypeStr {
  return curveTypeStr[curve]
}

export function curveTypeFromString(str: CurveTypeStr): Info.CurveType {
  return curveTypeStr.indexOf(str)
}

export function fromFixedNum(x: string, floating: number): number {
  return parseFloat(x) * Math.pow(2, floating)
}
export function toFixedNum(x: number, floating: number): string {
  return (x / Math.pow(2, floating)).toString()
}

export function coordToJson(coord: Info.Coord, floating: number): MapDir.Point<string> {
  return {
    x: toFixedNum(coord.x, floating),
    y: toFixedNum(coord.y, floating),
  }
}

export function coordFromJson(coord: MapDir.Point<string>, floating: number): Info.Coord {
  return {
    x: fromFixedNum(coord.x, floating),
    y: fromFixedNum(coord.y, floating),
  }
}

export function colorToJson(coord: Info.Color, floating: number): MapDir.Color<string> {
  return {
    r: toFixedNum(coord.r, floating),
    g: toFixedNum(coord.g, floating),
    b: toFixedNum(coord.b, floating),
    a: toFixedNum(coord.a, floating),
  }
}

export function colorFromJson(coord: MapDir.Color<string>, floating: number): Info.Color {
  return {
    r: fromFixedNum(coord.r, floating),
    g: fromFixedNum(coord.g, floating),
    b: fromFixedNum(coord.b, floating),
    a: fromFixedNum(coord.a, floating),
  }
}

// see https://developer.mozilla.org/en-US/docs/Glossary/Base64
function base64ToBytes(base64: string): Uint8Array {
  const binString = window.atob(base64);
  return Uint8Array.from(binString, (m) => m.codePointAt(0));
}

function bytesToBase64(bytes: Uint8Array): string {
  const binString = Array.from(bytes, (x) => String.fromCodePoint(x)).join("");
  return window.btoa(binString);
}

export function tilesToData(tiles: EditTileParams[]): string {
  let arr = []

  for (const tile of tiles) {
    if (tile.kind === 'tiles' || tile.kind === 'game' || tile.kind === 'front') {
      arr.push(tile.id, tile.flags, 0, 0)
    }
    else if (tile.kind === 'tele') {
      arr.push(tile.number, tile.id)
    }
    else if (tile.kind === 'speedup') {
      arr.push(tile.force, tile.maxSpeed, tile.id, 0, tile.angle & 0xff, (tile.angle >> 8) & 0xff) // little endian for angle
    }
    else if (tile.kind === 'switch') {
      arr.push(tile.number, tile.id, tile.flags, tile.delay)
    }
    else if (tile.kind === 'tune') {
      arr.push(tile.number, tile.id)
    }
    else {
      throw 'unsupported tile type'
    }
  }

  return bytesToBase64(new Uint8Array(arr))
}

export function tileToData(tile: EditTileParams): string {
  return tilesToData([tile])
}

export function dataToTiles(data: string, kind: MapDir.LayerKind): EditTileParams[] {
  const arr = base64ToBytes(data).buffer

  if (kind === 'tiles' || kind === 'game' || kind === 'front') {
    return Parser.parseTiles(arr, arr.byteLength / 4)
      .map(t => ({ kind, ...t }))
  }
  else if (kind === 'tele') {
    return Parser.parseTeleTiles(arr, arr.byteLength / 2)
      .map(t => ({ kind, ...t }))
  }
  else if (kind === 'speedup') {
    return Parser.parseSpeedupTiles(arr, arr.byteLength / 6)
      .map(t => ({ kind, ...t }))
  }
  else if (kind === 'switch') {
    return Parser.parseSwitchTiles(arr, arr.byteLength / 4)
      .map(t => ({ kind, ...t }))
  }
  else if (kind === 'tune') {
    return Parser.parseTuneTiles(arr, arr.byteLength / 2)
      .map(t => ({ kind, ...t }))
  }
  else {
    throw 'unsupported tile type ' + kind
  }
}
