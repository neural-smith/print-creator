// Version this alongside the application. Never intercept requests outside /app/.
const CACHE='print-creator-v0.5.0';
const PRECACHE=['./','./index.html','./app.js','./style.css','./manifest.webmanifest','./icon.png','./pkg/print_creator.js','./pkg/print_creator_bg.wasm'];
self.addEventListener('install',event=>{
  event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(PRECACHE)).then(()=>self.skipWaiting()));
});
self.addEventListener('activate',event=>{
  event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k.startsWith('print-creator-')&&k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim()));
});
self.addEventListener('fetch',event=>{
  if(event.request.method!=='GET'||new URL(event.request.url).origin!==self.location.origin)return;
  event.respondWith(caches.match(event.request).then(hit=>hit||fetch(event.request)));
});
