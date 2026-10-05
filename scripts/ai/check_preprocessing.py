"""Compare the single Rust preprocessing implementation to the pinned upstream processor.
Run ai-probe with the sample JPEGs first to produce <policy>.f32/features.f32.
"""
import argparse,json,pathlib
import numpy as np,torch
from PIL import Image
from transformers import CLIPModel,CLIPProcessor
p=argparse.ArgumentParser();p.add_argument('--probe',type=pathlib.Path,required=True);p.add_argument('images',type=pathlib.Path,nargs='+');args=p.parse_args()
config=json.loads((args.probe/'probe.json').read_text())
model=CLIPModel.from_pretrained(config['model_id'],revision=config['revision']).eval()
processor=CLIPProcessor.from_pretrained(config['model_id'],revision=config['revision'])
concepts=np.array([c['embedding'] for c in config['concepts']],dtype=np.float32)
rows=[]
for path in args.images:
 image=Image.open(path).convert('RGB'); image.thumbnail((384,384),Image.Resampling.LANCZOS)
 for policy in ('CenterCrop','Fit'):
  if policy=='CenterCrop':ref=processor(images=image,return_tensors='pt')['pixel_values'].numpy()
  else:
   w,h=image.size; nw=max(1,w*224//max(w,h));nh=max(1,h*224//max(w,h)); resized=np.asarray(image.resize((nw,nh),Image.Resampling.BICUBIC),dtype=np.float32)/255
   mean=np.array(config['processor']['image_mean'],dtype=np.float32);std=np.array(config['processor']['image_std'],dtype=np.float32)
   normal=(resized-mean)/std; fit=np.zeros((224,224,3),dtype=np.float32);ox=(224-nw)//2;oy=(224-nh)//2;fit[oy:oy+nh,ox:ox+nw]=normal;ref=fit.transpose(2,0,1)[None]
  actual=np.fromfile(path.with_suffix('.'+policy+'.f32'),dtype='<f4').reshape(1,3,224,224)
  rust_features=np.fromfile(path.with_suffix('.'+policy+'.features.f32'),dtype='<f4')
  with torch.no_grad():ref_features=model.get_image_features(pixel_values=torch.from_numpy(ref.copy())).numpy().ravel()
  ref_scores=concepts@(ref_features/np.linalg.norm(ref_features));rust_scores=concepts@(rust_features/np.linalg.norm(rust_features))
  rows.append({'image':path.name,'policy':policy,'input_max_abs':float(np.max(np.abs(actual-ref))),'input_mean_abs':float(np.mean(np.abs(actual-ref))),'feature_max_abs':float(np.max(np.abs(rust_features-ref_features))),'score_max_abs':float(np.max(np.abs(ref_scores-rust_scores))),'quality_validated':False})
print(json.dumps(rows,indent=2));(args.probe/'preprocessing-parity.json').write_text(json.dumps(rows,indent=2))
