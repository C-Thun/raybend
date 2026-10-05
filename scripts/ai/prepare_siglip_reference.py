"""Pinned SigLIP 2 comparison, not a distributable RayBend default model pack.
The encoder is exported separately by the same tools environment; preserve its hash.
"""
import argparse,json,pathlib,hashlib
import torch
from transformers import SiglipModel,AutoProcessor
p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--tiny-reference',type=pathlib.Path,required=True);args=p.parse_args()
model_id='google/siglip2-base-patch16-224';revision='75de2d55ec2d0b4efc50b3e9ad70dba96a7b2fa2'
model=SiglipModel.from_pretrained(model_id,revision=revision).eval();processor=AutoProcessor.from_pretrained(model_id,revision=revision)
concepts=json.loads(args.tiny_reference.read_text())['concepts'];out=args.output
with torch.no_grad():
 for concept in concepts:
  features=model.get_text_features(**processor(text=concept['prompts'],padding='max_length',max_length=64,truncation=True,return_tensors='pt'))
  features=features/features.norm(dim=-1,keepdim=True);feature=features.mean(dim=0);feature=feature/feature.norm();concept['embedding']=feature.numpy().tolist()
 x=torch.linspace(-2,2,3*224*224).reshape(1,3,224,224);expected=model.vision_model(pixel_values=x).pooler_output.numpy()
x.numpy().astype('<f4').tofile(out/'parity-input.f32');expected.astype('<f4').tofile(out/'parity-expected.f32')
meta=json.loads((out/'probe.json').read_text());meta.update(model_id=model_id,revision=revision,opset=17,concepts=concepts,onnx_sha256=hashlib.sha256((out/'image_encoder.onnx').read_bytes()).hexdigest());(out/'probe.json').write_text(json.dumps(meta,indent=2));print('prepared fixed comparison reference',flush=True)
