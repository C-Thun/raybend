"""Fixed-revision comparison encoder export; no product Python dependency."""
import time,json,pathlib,torch,onnxruntime as ort,numpy as np
from transformers import SiglipModel,AutoProcessor
import argparse
p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,required=True);args=p.parse_args()
out=args.output;out.mkdir(parents=True,exist_ok=True);model_id='google/siglip2-base-patch16-224';revision='75de2d55ec2d0b4efc50b3e9ad70dba96a7b2fa2'
print('revision',revision,flush=True)
model=SiglipModel.from_pretrained(model_id,revision=revision).eval();processor=AutoProcessor.from_pretrained(model_id,revision=revision)
class Encoder(torch.nn.Module):
 def __init__(self,m):super().__init__();self.vision=m.vision_model
 def forward(self,pixel_values):return self.vision(pixel_values=pixel_values).pooler_output
encoder=Encoder(model).eval();x=torch.linspace(-2,2,3*224*224).reshape(1,3,224,224);path=out/'image_encoder.onnx'
print('exporting',flush=True);torch.onnx.export(encoder,(x,),str(path),input_names=['pixel_values'],output_names=['image_features'],opset_version=17,dynamo=False)
options=ort.SessionOptions();options.intra_op_num_threads=2;options.inter_op_num_threads=1;t=time.monotonic();session=ort.InferenceSession(str(path),sess_options=options,providers=['CPUExecutionProvider']);load=time.monotonic()-t
ref=encoder(x).detach().numpy();actual=session.run(None,{'pixel_values':x.numpy()})[0];times=[]
for _ in range(30):
 t=time.monotonic();session.run(None,{'pixel_values':x.numpy()});times.append(time.monotonic()-t)
result={'model_id':model_id,'revision':revision,'bytes':path.stat().st_size,'load_s':load,'p50_s':float(np.median(times)),'p95_s':float(np.percentile(times,95)),'max_abs_error':float(np.max(np.abs(ref-actual))),'shape':list(actual.shape),'processor':processor.image_processor.to_dict(),'quality_validated':False}
(out/'probe.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2),flush=True)
