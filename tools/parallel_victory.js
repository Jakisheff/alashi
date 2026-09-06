// Staged winner reveal requested by the owner; never changes archive results.
const VictoryWorld=(()=>{
 const T=THREE,scene=new T.Scene();scene.background=new T.Color('#03060b');
 const renderer=new T.WebGLRenderer({antialias:true,preserveDrawingBuffer:true});renderer.setSize(1440,1080);renderer.outputColorSpace=T.SRGBColorSpace;renderer.toneMapping=T.ACESFilmicToneMapping;renderer.toneMappingExposure=1.15;
 const camera=new T.PerspectiveCamera(43,1440/1080,.1,200);camera.position.set(7,6.5,19);camera.lookAt(0,.5,0);
 scene.add(new T.HemisphereLight(0xb3d3ec,0x080c16,1.3));
 const key=new T.DirectionalLight(0xffe3a1,3.4);key.position.set(-7,12,12);scene.add(key);const rim=new T.DirectionalLight(0x5abfff,3);rim.position.set(6,8,-7);scene.add(rim);
 const texture=new T.TextureLoader().load('__EARTH_TEXTURE__');texture.colorSpace=T.SRGBColorSpace;texture.anisotropy=renderer.capabilities.getMaxAnisotropy();
 const globe=new T.Mesh(new T.SphereGeometry(5,96,64),new T.MeshStandardMaterial({map:texture,roughness:.78,metalness:.08,emissive:0x356a91,emissiveMap:texture,emissiveIntensity:.65}));globe.position.y=-1;scene.add(globe);
 const atmosphere=new T.Mesh(new T.SphereGeometry(5.08,64,48),new T.ShaderMaterial({transparent:true,side:T.BackSide,depthWrite:false,uniforms:{tint:{value:new T.Color('#42bfff')}},vertexShader:'varying vec3 n;varying vec3 v;void main(){vec4 p=modelViewMatrix*vec4(position,1.);n=normalize(normalMatrix*normal);v=normalize(-p.xyz);gl_Position=projectionMatrix*p;}',fragmentShader:'varying vec3 n;varying vec3 v;uniform vec3 tint;void main(){float f=pow(1.-abs(dot(normalize(n),normalize(v))),3.);gl_FragColor=vec4(tint,f*.7);}'}));atmosphere.position.copy(globe.position);scene.add(atmosphere);
 const hero=StudioWorld.agents[archive.agents.findIndex(a=>a.name==='Aitore')].clone(true);hero.position.set(0,4.02,0);hero.scale.setScalar(1.25);scene.add(hero);
 const podium=new T.Mesh(new T.TorusGeometry(5.6,.035,8,128),new T.MeshBasicMaterial({color:'#d2b276'}));podium.rotation.x=Math.PI/2;podium.position.y=-6.05;scene.add(podium);
 const points=[];let seed=31;const rand=()=>{seed=(seed*1664525+1013904223)>>>0;return seed/4294967296};for(let i=0;i<800;i++)points.push((rand()-.5)*65,(rand()-.3)*28,-10-rand()*25);const geo=new T.BufferGeometry();geo.setAttribute('position',new T.Float32BufferAttribute(points,3));const crowd=new T.Points(geo,new T.PointsMaterial({color:'#aec9de',size:.035,transparent:true,opacity:.6}));scene.add(crowd);
 const beams=[];for(let i=0;i<5;i++){const material=new T.MeshBasicMaterial({color:i%2?'#54aee8':'#ead2a1',transparent:true,opacity:.035,depthWrite:false,side:T.DoubleSide,blending:T.AdditiveBlending});const beam=new T.Mesh(new T.ConeGeometry(1.7,23,24,1,true),material);beam.position.set((i-2)*4,5,-7);beam.rotation.z=(i-2)*.11;scene.add(beam);beams.push(beam)}
 function render(t){globe.rotation.y=.65+t*.025;hero.rotation.y=.3+Math.sin(t*.12)*.1;camera.position.set(7-t*.05,6.5,19-t*.07);camera.lookAt(0,.5,0);beams.forEach((b,i)=>b.rotation.z=(i-2)*.11+Math.sin(t*.22+i)*.06);renderer.render(scene,camera);return renderer.domElement}
 return {render,texture,hero,ready:()=>!!texture.image?.complete};
})();
function renderVictory(t){g.setTransform(1,0,0,1,0,0);g.fillStyle='#03060b';g.fillRect(0,0,1920,1080);g.drawImage(VictoryWorld.render(t-108),480,0,1440,1080);
 text('ALASHI',80,84,40,'#a9bdc7');text('ПОБЕДИТЕЛЬ',80,340,38,'#d7bd82',600);g.save();g.shadowColor='#dfc895';g.shadowBlur=22;text('AITORE',72,442,112,'#f3dfae',700);g.restore();g.fillStyle='#d7bd82';g.fillRect(80,529,360,2);text('ЕГО ХОД. ЕГО МИР.',80,592,26,'#c7d6dc');text('ПОСТАНОВОЧНЫЙ ФИНАЛ',80,990,18,'#899ca7');text('N.O.R.M. · No Church in the Wild (8 Bit Remix)',1840,1040,17,'#899ca7',500,'right');window.parallelLastTime=t;
}
