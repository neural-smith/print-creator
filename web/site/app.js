import init, {import_image, import_project, import_template_xml} from './pkg/print_creator.js';

const notice = document.getElementById('browser-message');
const photos = document.getElementById('photo-picker');
const projects = document.getElementById('project-picker');
const templates = document.getElementById('template-picker');
let running = false;
let hideMessage;

function showError(error) {
  notice.textContent = String(error?.message || error);
  clearTimeout(hideMessage);
  hideMessage = setTimeout(() => {notice.textContent = '';}, 9000);
}

window.pcOpenPhotos = () => photos.click();
window.pcOpenProject = () => projects.click();
window.pcOpenTemplates = () => templates.click();
window.pcDownload = (name, mime, bytes) => {
  const data = new Blob([bytes], {type: mime});
  const url = URL.createObjectURL(data);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.style.display = 'none';
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
};

const PHOTO_TYPES = new Set(['jpg','jpeg','png','tif','tiff','bmp','webp']);
async function addPhotos(files) {
  for (const file of files) {
    if (!PHOTO_TYPES.has((file.name.split('.').pop() || '').toLowerCase())) continue;
    try {
      if (file.size > 80 * 1024 * 1024) throw new Error(`${file.name} exceeds the 80 MiB limit`);
      const bytes = new Uint8Array(await file.arrayBuffer());
      import_image(file.name, bytes);
    } catch (error) {showError(`Cannot import ${file.name}: ${error}`);}
  }
}

photos.addEventListener('change', async () => {
  await addPhotos(Array.from(photos.files || []));
  photos.value = '';
});
projects.addEventListener('change', async () => {
  const file = projects.files?.[0];
  if (file) {
    try {import_project(new Uint8Array(await file.arrayBuffer()));}
    catch (error) {showError(error);}
  }
  projects.value = '';
});

function readEntries(reader) {
  return new Promise((resolve,reject) => reader.readEntries(resolve,reject));
}
function entryFile(entry) {
  return new Promise((resolve,reject) => entry.file(resolve,reject));
}
async function flattenEntry(entry, result, limit = 500) {
  if (result.length >= limit) return;
  if (entry.isFile) {
    result.push(await entryFile(entry));
  } else if (entry.isDirectory) {
    const reader=entry.createReader();
    while (result.length < limit) {
      const entries=await readEntries(reader);
      if (!entries.length) break;
      for (const child of entries) await flattenEntry(child,result,limit);
    }
  }
}
templates.addEventListener('change', async () => {
  const file = templates.files?.[0];
  if (file) {
    try {import_template_xml(await file.text());}
    catch (error) {showError(error);}
  }
  templates.value = '';
});

let dragDepth = 0;
window.addEventListener('dragenter',e=>{e.preventDefault();dragDepth++;document.body.classList.add('drop-active');});
window.addEventListener('dragleave',e=>{e.preventDefault();dragDepth=Math.max(0,dragDepth-1);if(!dragDepth)document.body.classList.remove('drop-active');});
window.addEventListener('dragover',e=>{e.preventDefault();});
window.addEventListener('drop',async e=>{
  e.preventDefault();dragDepth=0;document.body.classList.remove('drop-active');
  const list=[];
  const items=Array.from(e.dataTransfer?.items || []);
  try {
    if (items.some(item=>item.webkitGetAsEntry?.())) {
      for (const item of items) {
        const entry=item.webkitGetAsEntry?.();
        if (entry)await flattenEntry(entry,list);
      }
    } else list.push(...Array.from(e.dataTransfer?.files || []));
    await addPhotos(list);
  } catch(error) {showError(error);}
});

try {
  await init();
  running=true;
  if ('serviceWorker' in navigator && location.protocol === 'https:') {
    navigator.serviceWorker.register('./sw.js').catch(console.warn);
  }
} catch(error) {
  document.getElementById('startup-status').textContent = `WebAssembly startup failed: ${error}`;
  showError(error);
}
