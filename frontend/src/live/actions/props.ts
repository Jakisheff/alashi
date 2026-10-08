import { BoxGeometry, CylinderGeometry, Group, Mesh, MeshStandardMaterial, SphereGeometry, TorusGeometry, type BufferGeometry } from 'three'

/** Every prop is owned here; no cached GLB geometry/material is mutated or disposed. */
export function createScenarioProps() {
  const root = new Group()
  root.name = 'scenario-props'
  const geometries: BufferGeometry[] = []
  const make = <T extends BufferGeometry>(geometry: T) => { geometries.push(geometry); return geometry }
  const cube = make(new BoxGeometry(1, 1, 1))
  const ball = make(new SphereGeometry(1, 20, 12))
  const cylinder = make(new CylinderGeometry(1, 1, 1, 24))
  const ring = make(new TorusGeometry(1, .06, 8, 32))
  const dome = make(new SphereGeometry(1, 24, 12, 0, Math.PI * 2, 0, Math.PI / 2))
  const palette = {
    wood: '#75472d', edge: '#bc8150', grain: '#93603c', cream: '#e6ddbf', paper: '#f3e8ce',
    teal: '#173f38', cloth: '#96734c', cord: '#dac098', ink: '#203934', gold: '#ddaa45', wax: '#bc512e',
    dark: '#202b2b', sleeve: '#303d40', skin: '#b8aa8b', slot: '#121d1b',
  }
  const materials = Object.fromEntries(Object.entries(palette).map(([key, color]) => [key,
    new MeshStandardMaterial({ color, roughness: key === 'gold' ? .32 : .72, metalness: key === 'gold' ? .65 : 0, transparent: true }),
  ])) as Record<keyof typeof palette, MeshStandardMaterial>
  type Finish = keyof typeof palette
  const group = (name: string, parent = root) => { const g = new Group(); g.name = name; parent.add(g); return g }
  function mesh(parent: Group, geometry: BufferGeometry, color: Finish, pos: [number, number, number], scale: [number, number, number], name = '') {
    const m = new Mesh(geometry, materials[color]); m.position.set(...pos); m.scale.set(...scale); m.name = name
    parent.add(m); return m
  }
  const box = (g: Group, color: Finish, pos: [number, number, number], size: [number, number, number], name = '') => mesh(g, cube, color, pos, size, name)
  const sphere = (g: Group, color: Finish, pos: [number, number, number], size: [number, number, number]) => mesh(g, ball, color, pos, size)

  const bag = group('contraband-sack')
  sphere(bag, 'cloth', [0, .265, 0], [.22, .265, .205])
  mesh(bag, cylinder, 'cloth', [0, .50, 0], [.105, .12, .09])
  const tie = mesh(bag, ring, 'cord', [0, .49, 0], [.112, .1, .09]); tie.rotation.x = Math.PI / 2
  for (const x of [-.07, .07]) {
    const string = box(bag, 'cord', [x, .405, .18], [.018, .15, .018]); string.rotation.z = x * 3
  }
  for (const x of [-.12, 0, .12]) box(bag, 'grain', [x, .23, .199], [.008, .22, .008])
  const parcel = group('concealed-parcel')
  box(parcel, 'edge', [0, .09, 0], [.22, .18, .19], 'parcel-solid')
  box(parcel, 'cord', [0, .09, .097], [.026, .18, .008])
  box(parcel, 'cord', [0, .184, 0], [.026, .008, .19])
  const hat = group('sneaky-knit-cap')
  mesh(hat, dome, 'dark', [0, 0, 0], [.66, .25, .48])
  mesh(hat, cylinder, 'dark', [0, .012, 0], [.66, .06, .48])
  for (let i = -5; i <= 5; i++) box(hat, 'sleeve', [i * .09, .025, .477 * Math.sqrt(1 - (i * .09 / .67) ** 2)], [.012, .062, .009])

  const desk = group('official-desk')
  // Top y=0: fingers and envelope remain above this plane during the exchange.
  box(desk, 'wood', [0, -.45, 0], [.76, .85, .61])
  box(desk, 'edge', [0, -.055, 0], [.9, .11, .74], 'desktop')
  box(desk, 'gold', [0, -.24, .316], [.3, .085, .018])
  for (const y of [-.38, -.66]) box(desk, 'grain', [0, y, .311], [.66, .018, .014])
  for (const z of [-.22, 0, .22]) box(desk, 'grain', [0, .002, z], [.85, .005, .01])
  const official = group('faceless-official')
  sphere(official, 'sleeve', [0, .30, 0], [.24, .35, .17])
  sphere(official, 'dark', [0, .78, 0], [.14, .18, .13])
  mesh(official, cylinder, 'dark', [0, .91, 0], [.19, .055, .16])
  box(official, 'gold', [-.07, .43, .153], [.06, .07, .014])
  box(official, 'cream', [0, .50, .16], [.075, .09, .015])
  const officialHand = group('official-receiving-hand')
  box(officialHand, 'sleeve', [.16, -.055, -.1], [.32, .14, .22])
  box(officialHand, 'cream', [0, -.047, -.1], [.07, .12, .22])
  sphere(officialHand, 'skin', [-.095, -.043, .016], [.145, .043, .13])
  // Origin is the top of the receiving palm, not the middle of its volume.
  for (let i = 0; i < 4; i++) sphere(officialHand, 'skin', [-.155 + i * .048, -.035, .145], [.025, .035, .085])
  const envelope = group('sealed-envelope')
  box(envelope, 'paper', [0, .025, 0], [.38, .05, .27], 'envelope-solid')
  for (const x of [-.09, .09]) {
    const fold = box(envelope, 'cord', [x, .051, 0], [.23, .003, .008]); fold.rotation.y = x > 0 ? -.6 : .6
  }
  mesh(envelope, cylinder, 'wax', [0, .059, .025], [.043, .016, .043])

  const urn = group('ballot-box')
  // True hollow box and a split lid: the paper drops through an actual slot.
  box(urn, 'wood', [0, -.61, 0], [.68, .08, .55])
  box(urn, 'wood', [0, -.32, .265], [.68, .58, .035])
  box(urn, 'wood', [0, -.32, -.265], [.68, .58, .035])
  for (const x of [-.322, .322]) box(urn, 'edge', [x, -.32, 0], [.035, .58, .5])
  for (const z of [-.1875, .1875]) box(urn, 'edge', [0, -.04, z], [.76, .08, .205])
  for (const x of [-.31, .31]) box(urn, 'edge', [x, -.04, 0], [.14, .08, .17])
  // Slot clear opening x=±.24,z=±.085; ballot x=±.15,z=±.008.
  box(urn, 'dark', [0, -.52, 0], [.6, .015, .46])
  box(urn, 'gold', [0, -.28, .287], [.35, .29, .012])
  const check1 = box(urn, 'ink', [-.055, -.30, .3], [.115, .027, .012]); check1.rotation.z = -.7
  const check2 = box(urn, 'ink', [.04, -.26, .3], [.20, .027, .012]); check2.rotation.z = .85
  const ballot = group('ballot-paper')
  box(ballot, 'paper', [0, .17, 0], [.30, .34, .016], 'ballot-solid')
  box(ballot, 'gold', [0, .27, .009], [.23, .018, .002])
  const tick1 = box(ballot, 'teal', [-.04, .15, .01], [.085, .018, .002]); tick1.rotation.z = -.65
  const tick2 = box(ballot, 'teal', [.025, .175, .01], [.13, .018, .002]); tick2.rotation.z = .8

  const receiver = group('mule-receiving-palm')
  box(receiver, 'dark', [-.2, -.09, -.04], [.4, .16, .24])
  sphere(receiver, 'cloth', [0, -.045, .02], [.19, .045, .19])

  function fade(opacity: number) {
    root.visible = opacity > .001
    for (const material of Object.values(materials)) { material.opacity = opacity; material.depthWrite = opacity > .98 }
  }
  function hideAll() {
    for (const child of root.children) { child.visible = false; child.position.set(0, 0, 0); child.rotation.set(0, 0, 0); child.scale.setScalar(1) }
  }
  function dispose() { geometries.forEach((g) => g.dispose()); Object.values(materials).forEach((m) => m.dispose()) }
  return { root, bag, parcel, hat, desk, official, officialHand, envelope, urn, ballot, receiver, fade, hideAll, dispose }
}
export type ScenarioProps = ReturnType<typeof createScenarioProps>
