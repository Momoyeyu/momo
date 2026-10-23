import test from 'node:test'
import assert from 'node:assert/strict'
import * as motion from '../docs/.vitepress/theme/meeting-motion.mjs'
const target={left:640,top:250,width:650,height:440}, viewport={width:1440,height:1000}
test('the room reaches its measured DOM rectangle while the opening stays behind',()=>{
  const frame=motion.meetingFrame(1,target,viewport)
  assert.equal(frame.x,0);assert.equal(frame.y,0)
  assert.equal(frame.scale,1);assert.equal(frame.rotation,-1.6)
  assert.equal(frame.reveal,1);assert.equal(frame.landing,1)
  assert.equal(frame.intro,0);assert.equal(frame.chapter,1)
})
test('scroll direction is reversible and the stable midpoint is independent of elapsed time',()=>{
  const forward=[0,.1,.3,.6,1].map(p=>motion.meetingFrame(p,target,viewport))
  const reverse=[1,.6,.3,.1,0].map(p=>motion.meetingFrame(p,target,viewport)).reverse()
  assert.deepEqual(forward,reverse)
  assert.equal(forward[0].reveal,0);assert.equal(forward[0].presence,1)
  assert.equal(forward[2].presence,0)
  assert.ok(forward[3].scale>1)
  assert.deepEqual(motion.meetingFrame(-20,target,viewport),forward[0])
  assert.deepEqual(motion.meetingFrame(20,target,viewport),forward[4])
})
test('transparent cells never become particles and ink identities survive repeated sampling',()=>{
  assert.equal(typeof motion.presenceCells,'function','presence is sampled once into stable ink cells')
  const image={width:2,height:2,data:new Uint8ClampedArray([243,236,223,255,231,190,119,255,24,21,31,255,243,236,223,0])}
  const cells=motion.presenceCells(image)
  assert.equal(cells.length,3)
  assert.deepEqual(cells.map(c=>[c.id,c.u,c.v,c.ink]),[[0,.25,.25,0],[1,.75,.25,1],[2,.25,.75,2]])
  assert.deepEqual(motion.presenceCells(image),cells)
})
test('the same glyph travels from its silhouette cell into the room and leaves no particles over the video',()=>{
  assert.equal(typeof motion.presenceCellFrame,'function','stable cells carry the transformation')
  const cell={id:7,u:.25,v:.75,ink:1,seed:.4,roomU:.8,roomV:.2}
  const source={left:10,top:20,width:400,height:600}, room={left:500,top:200,width:600,height:400}
  const first=motion.presenceCellFrame(cell,.12,source,room)
  assert.equal(first.x,110);assert.equal(first.y,470)
  const travelling=motion.presenceCellFrame(cell,.35,source,room)
  assert.ok(travelling.x>110 && travelling.x<980)
  assert.ok(travelling.alpha>0)
  const arrived=motion.presenceCellFrame(cell,.6,source,room)
  assert.equal(arrived.x,980);assert.equal(arrived.y,280)
  assert.equal(motion.presenceCellFrame(cell,1,source,room).alpha,0)
  assert.deepEqual(motion.presenceCellFrame(cell,.35,source,room),travelling)
})
test('room blocks decode in a spatial scan and finish without seams before video playback',()=>{
  assert.equal(typeof motion.roomCellFrame,'function','room decodes through a diagonal spatial cascade')
  const early={u:.05,v:.05,seed:.2}, late={u:.95,v:.95,seed:.2}
  assert.equal(motion.roomCellFrame(early,0).reveal,0)
  assert.ok(motion.roomCellFrame(early,.45).reveal>motion.roomCellFrame(late,.45).reveal)
  assert.equal(motion.roomCellFrame(late,.7).reveal,1)
  assert.equal(motion.roomCellFrame(late,1).alpha,0)
})
test('every retained source cell has a distinct destination even when the cell count shares a factor with the permutation',()=>{
  const data=new Uint8ClampedArray(73*4)
  for(let i=0;i<73;i++)data.set([231,190,119,255],i*4)
  const cells=motion.presenceCells({width:73,height:1,data})
  assert.equal(new Set(cells.map(cell=>`${cell.roomU},${cell.roomV}`)).size,73)
})
test('centered cover keeps known source coordinates when the destination is taller or wider',()=>{
  assert.equal(typeof motion.coverCrop,'function','the pixel room must use the same centered cover crop as the real poster')
  // A 16:9 source in a 4:3 frame discards one eighth from each horizontal edge.
  assert.deepEqual(motion.coverCrop(16/9,4/3),{left:.125,top:0,width:.75,height:1})
  // A 4:3 source in a 16:9 frame instead discards one eighth from top and bottom.
  assert.deepEqual(motion.coverCrop(4/3,16/9),{left:0,top:.125,width:1,height:.75})
  assert.deepEqual(motion.coverCrop(16/9,16/9),{left:0,top:0,width:1,height:1})
})
test('video handoff keeps the poster opaque until the live surface is fully revealed',()=>{
  const cell={u:.5,v:.5,seed:.2}
  assert.equal(motion.roomCellFrame(cell,.699).alpha,1)
  assert.equal(motion.roomCellFrame(cell,.7).alpha,0)
  assert.equal(motion.meetingFrame(.7,target,viewport).reveal,1)
})
