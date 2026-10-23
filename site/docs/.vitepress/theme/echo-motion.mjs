import { phase } from './chapter-motion.mjs'

// Two offset impressions share a rhythm briefly, then separate again.
// Scroll is the only clock: reversing retraces the same ink, including its grain.
export function sampleEcho(progress, width, height) {
  const p = Math.min(.8, Math.max(0, progress))
  const input = phase(p, .04, .32)
  const reply = phase(p, .43, .73)
  const question = phase(p, .70, .8)
  const settle = phase(p, .63, .8)
  const count = width < 480 ? 42 : 60
  const rows = width < 480 ? 7 : 9
  const glyphs = []
  for (let plane = 0; plane < 2; plane++) {
    for (let row = 0; row < rows; row++) for (let col = 0; col < count; col++) {
      const u = col / (count - 1), lane = row / (rows - 1) - .5
      const seed = Math.sin(col*127.1 + row*311.7 + plane*43.3)*43758.5453
      const grain = seed - Math.floor(seed)
      const arrival = plane ? phase(p,.43+(1-u)*.10,.53+(1-u)*.10) : phase(p,.04+u*.12,.16+u*.12)
      const offset = plane ? (1-reply)*2.4 : .8-settle*.3
      const fold = u*7.6 + lane*1.8 + offset
      const envelope = Math.pow(Math.sin(Math.PI*u), .65)
      const x = width*(.035 + .93*u + Math.sin(fold*.7)*lane*.045*reply)
      const y = height*(.49 + Math.sin(fold)*(.17 + lane*.05) + lane*.38*envelope + (u-.5)*.16*settle)
      const residue = plane ? 1-settle*.15 : 1-phase(p,.27,.43)*.76
      const depth = .22 + .78*Math.pow(Math.cos(lane*2),2)
      const opacity = arrival * residue * envelope * depth * (plane ? .45+grain*.5 : .2+grain*.45)
      glyphs.push({x,y,opacity,gold:!!plane,char:'01:·/—;.'[(col*7+row*3+plane)%8],
        size: grain>.88 ? 12 : 10, thread: plane*rows+row, start: col===0})
    }
  }
  return {input,reply,question,glyphs}
}
