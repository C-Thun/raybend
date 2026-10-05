"""Legacy research entry: share the registry encoder exporter, keep reference vectors here."""
import argparse, json, pathlib, subprocess, sys

_CONTRACT = json.loads((pathlib.Path(__file__).resolve().parents[2] / 'crates/raybend/assets/ai/tinyclip-v1/manifest.json').read_text())
MODEL_ID, REVISION = _CONTRACT['model_id'], _CONTRACT['revision']

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--registry', type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[3] / 'model-registry')
    args = parser.parse_args()
    exporter = args.registry / 'scripts/export-tinyclip.py'
    recipe_path = args.registry / 'recipes/tinyclip-v1/recipe.json'
    if not exporter.is_file() or not recipe_path.is_file():
        parser.error('先签出 model-registry，或用 --registry 指定工具库路径')
    subprocess.run([sys.executable, str(exporter), '--output', str(args.output), '--recipe', str(recipe_path)], check=True)
    import torch
    from transformers import CLIPModel, CLIPProcessor
    recipe = json.loads(recipe_path.read_text())
    model_id, revision = recipe['source']['modelId'], recipe['source']['revision']
    if (model_id, revision) != (MODEL_ID, REVISION):
        raise ValueError('registry checkpoint differs from this research contract')
    model = CLIPModel.from_pretrained(model_id, revision=revision).eval()
    processor = CLIPProcessor.from_pretrained(model_id, revision=revision)
    labels = {'person':['a photo of a person','a photo of people'], 'bird':['a photo of a bird','a photo of birds'], 'car':['a photo of a car','a photo of cars'], 'mountain':['a photo of mountains','a photo of a mountain'], 'sea':['a photo of the ocean','a photo of the sea'], 'forest':['a photo of a forest','a photo of woodland'], 'night':['a photo of a night scene','a photo of a city at night']}
    concepts = []
    with torch.no_grad():
        for key, prompts in labels.items():
            vector = model.get_text_features(**processor(text=prompts,padding=True,return_tensors='pt'))
            vector = vector / vector.norm(dim=-1,keepdim=True)
            vector = vector.mean(dim=0); vector = vector / vector.norm()
            concepts.append(dict(key=key,prompts=prompts,embedding=vector.numpy().tolist()))
    probe = json.loads((args.output / 'probe.json').read_text())
    probe.update(model_id=model_id,revision=revision,onnx_sha256=probe['model_sha256'],bytes=recipe['model']['bytes'],processor=processor.image_processor.to_dict(),concepts=concepts,quality_validated=False)
    (args.output / 'probe.json').write_text(json.dumps(probe,indent=2)+'\n')

if __name__ == '__main__': main()
