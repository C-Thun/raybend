"""Freeze reviewed threshold metadata. Does not trust a generated manifest automatically or distribute weights."""
import argparse, hashlib, json, pathlib
EXPECTED_MODEL = 'a8576f0f9916d6e92237996d41c256e1e3150db5cf6db48758bb63c87537bb24'
NAMES = {'person':'人','bird':'鸟','car':'车','mountain':'山','sea':'海','forest':'森林','night':'夜景'}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--reference', type=pathlib.Path, required=True)
    parser.add_argument('--quality', type=pathlib.Path, required=True)
    parser.add_argument('--license', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    reference = json.loads((args.reference / 'probe.json').read_text())
    quality = json.loads(args.quality.read_text())
    if reference['revision'] != '95ec8197b3f2fe7f747865c61ca556cf0768b2f7' or reference['onnx_sha256'] != EXPECTED_MODEL:
        raise ValueError('unreviewed checkpoint')
    policy = quality.get('threshold_policy')
    if policy != dict(precision_target=.85,beta=.5,min_tp=5,selected_on='calibration only',strict_previous_gate_retained=True):
        raise ValueError('threshold policy differs from the agreed precision-first policy')
    report = quality['policies']['CenterCrop']['classes']
    classes = []
    for concept in reference['concepts']:
        key = concept['key']; threshold = report[key]['threshold']
        if threshold is None: raise ValueError(f'{key} lacks a supported threshold')
        classes.append(dict(concept=concept,zh=NAMES[key],en=key,threshold=threshold))
    if {c['concept']['key'] for c in classes} != set(NAMES): raise ValueError('core vocabulary differs')
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'classes.json').write_text(json.dumps(classes,ensure_ascii=False,indent=2)+'\n')
    (args.output / 'LICENSE.txt').write_bytes(args.license.read_bytes())
    files = []
    for name in ['image_encoder.onnx','classes.json','LICENSE.txt']:
        path = args.reference / name if name.endswith('.onnx') else args.output / name
        data = path.read_bytes()
        files.append(dict(name=name,bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
    if files[0]['sha256'] != EXPECTED_MODEL: raise ValueError('model bytes differ')
    manifest = dict(format=2,model_id=reference['model_id'],revision=reference['revision'],ort_version='1.28.0',opset=17,image_input='pixel_values',image_output='image_features',resize='center_crop',thresholds_calibrated=True,files=files)
    raw = (json.dumps(manifest,ensure_ascii=False,indent=2)+'\n').encode()
    (args.output/'manifest.json').write_bytes(raw)
    print(hashlib.sha256(raw).hexdigest())

if __name__ == '__main__': main()
