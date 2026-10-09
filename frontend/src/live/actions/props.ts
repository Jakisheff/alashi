import { createBrandCoin } from '../../brand/coin.ts'
import { DoubleSide, MeshBasicMaterial, PlaneGeometry, BoxGeometry, CylinderGeometry, Group, Mesh, MeshStandardMaterial, SphereGeometry, TorusGeometry, type Texture, type BufferGeometry } from 'three'

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

  const cutoutMaterial = new MeshBasicMaterial({ transparent: true, alphaTest: .02, side: DoubleSide, toneMapped: false, depthWrite: false })
  const donkeyBillboard = group('reference-donkey')
  const donkeyPicture = new Mesh(make(new PlaneGeometry(1, 1)), cutoutMaterial)
  donkeyPicture.name = 'masked-donkey-cutout'; donkeyBillboard.add(donkeyPicture)
  let hasDonkeyTexture = false
  function setDonkeyTexture(texture: Texture | null) {
    hasDonkeyTexture = texture !== null
    cutoutMaterial.map = texture; cutoutMaterial.needsUpdate = true
  }
  const brandCoin = createBrandCoin(.12, .035, .125)
  const coin = brandCoin.root; root.add(coin)
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
  const official = group('discreet-official')
  const officialTorso = group('official-torso', official)
  sphere(officialTorso, 'sleeve', [0, .30, 0], [.23, .34, .17])
  box(officialTorso, 'cream', [0, .49, .165], [.13, .16, .025])
  const tieKnot = box(officialTorso, 'wax', [0, .51, .185], [.045, .045, .02]); tieKnot.rotation.z = Math.PI / 4
  box(officialTorso, 'wax', [0, .42, .18], [.04, .12, .02])
  box(officialTorso, 'gold', [-.12, .40, .158], [.06, .075, .022])
  for (const y of [.20, .29]) sphere(officialTorso, 'gold', [0, y, .164], [.015, .015, .012])
  const officialHead = group('official-head', officialTorso); officialHead.position.set(0, .67, 0)
  sphere(officialHead, 'skin', [0, .08, 0], [.155, .19, .135])
  for (const x of [-.16, .16]) sphere(officialHead, 'skin', [x, .06, 0], [.035, .055, .03])
  sphere(officialHead, 'skin', [0, .055, .137], [.04, .055, .055])
  for (const x of [-.069, .069]) {
    sphere(officialHead, 'paper', [x, .10, .12], [.055, .047, .025])
    sphere(officialHead, 'dark', [x, .095, .144], [.019, .022, .012])
    mesh(officialHead, ring, 'gold', [x, .10, .145], [.062, .05, .04])
    const brow = box(officialHead, 'dark', [x, .157, .121], [.085, .02, .018]); brow.rotation.z = x * -1.8
    const moustache = sphere(officialHead, 'dark', [x * .5, .01, .14], [.052, .02, .022]); moustache.rotation.z = x * 2
  }
  box(officialHead, 'gold', [0, .10, .147], [.027, .011, .009])
  mesh(officialHead, cylinder, 'sleeve', [0, .255, 0], [.205, .09, .18])
  box(officialHead, 'dark', [0, .20, .10], [.30, .025, .21])
  box(officialHead, 'gold', [0, .247, .178], [.065, .047, .012])
  const officialHand = group('official-receiving-hand')
  // Horizontal palm, origin at its contact plane. Connected sleeves are posed below.
  sphere(officialHand, 'skin', [0, -.035, 0], [.10, .035, .095])
  for (let i = 0; i < 4; i++) sphere(officialHand, 'skin', [-.075 + i * .05, -.025, .103], [.023, .025, .055])
  sphere(officialHand, 'skin', [-.10, -.025, .03], [.026, .026, .05])
  const officialUpperArm = group('official-upper-arm')
  mesh(officialUpperArm, cylinder, 'sleeve', [0, .5, 0], [.075, 1, .075])
  const officialForearm = group('official-forearm')
  mesh(officialForearm, cylinder, 'sleeve', [0, .45, 0], [.066, .9, .066])
  mesh(officialForearm, cylinder, 'cream', [0, .95, 0], [.071, .1, .071])
  const officialOtherArm = group('official-other-arm', officialTorso)
  sphere(officialOtherArm, 'sleeve', [.22, .27, .02], [.075, .23, .075])
  sphere(officialOtherArm, 'skin', [.23, .05, .02], [.07, .075, .07])
  const coverHand = group('official-cover-hand')
  sphere(coverHand, 'skin', [0, .028, 0], [.075, .028, .07])
  for (let i = 0; i < 4; i++) sphere(coverHand, 'skin', [-.055 + i * .036, .018, .06], [.017, .018, .04])
  const coverUpperArm = group('official-cover-upper-arm')
  mesh(coverUpperArm, cylinder, 'sleeve', [0, .5, 0], [.066, 1, .066])
  const coverForearm = group('official-cover-forearm')
  mesh(coverForearm, cylinder, 'sleeve', [0, .45, 0], [.06, .9, .06])
  mesh(coverForearm, cylinder, 'cream', [0, .95, 0], [.065, .1, .065])
  // The envelope goes under an entirely unconvincing stack of routine paperwork.
  const paperwork = group('official-paperwork')
  for (let i = 0; i < 3; i++) {
    const sheet = box(paperwork, 'paper', [i * .015, .008 + i * .018, 0], [.40, .014, .28]); sheet.rotation.y = (i - 1) * .08
  }
  box(paperwork, 'ink', [0, .055, -.065], [.25, .003, .012])
  box(paperwork, 'ink', [-.03, .055, -.025], [.19, .003, .009])
  const envelope = group('sealed-envelope')
  box(envelope, 'paper', [0, .025, 0], [.46, .05, .27], 'envelope-solid')
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

  function fade(opacity: number) {
    brandCoin.fade(opacity)
    cutoutMaterial.opacity = opacity
    root.visible = opacity > .001
    for (const material of Object.values(materials)) { material.opacity = opacity; material.depthWrite = opacity > .98 }
  }
  const articulated = [officialTorso, officialHead, officialOtherArm]
  const rests = articulated.map((node) => ({ node, visible: node.visible, position: node.position.clone(), quaternion: node.quaternion.clone(), scale: node.scale.clone() }))
  function hideAll() {
    for (const { node, visible, position, quaternion, scale } of rests) { node.visible = visible; node.position.copy(position); node.quaternion.copy(quaternion); node.scale.copy(scale) }
    for (const child of root.children) { child.visible = false; child.position.set(0, 0, 0); child.rotation.set(0, 0, 0); child.scale.setScalar(1) }
  }
  function dispose() { brandCoin.dispose(); cutoutMaterial.dispose(); geometries.forEach((g) => g.dispose()); Object.values(materials).forEach((m) => m.dispose()) }
  return { root, coin, donkeyBillboard, donkeyPicture, setDonkeyTexture, get hasDonkeyTexture() { return hasDonkeyTexture }, bag, parcel, hat, desk, official, officialTorso, officialHead, officialOtherArm, officialHand, officialUpperArm, officialForearm, coverHand, coverUpperArm, coverForearm, paperwork, envelope, urn, ballot, fade, hideAll, dispose }
}
export type ScenarioProps = ReturnType<typeof createScenarioProps>
