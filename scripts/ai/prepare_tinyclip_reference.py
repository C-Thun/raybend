"""One bounded prompt revision; uses the pinned exporter checkpoint, never trains on validation."""
import argparse, json, pathlib, shutil
import torch
from transformers import CLIPModel, CLIPProcessor
from export_tinyclip import MODEL_ID, REVISION

PROMPTS = {
    'person': ['a photo of a person', 'a photo of people', 'a photo of a man', 'a photo of a woman', 'a photo of a child', 'a portrait photo of a person'],
    'bird': ['a photo of a bird', 'a photo of birds'],
    'car': ['a photo of a car', 'a photo of cars'],
    'mountain': ['a photo of mountains', 'a photo of a mountain'],
    'sea': ['a photo of the sea', 'a photo of ocean waves', 'a photo of a sea coast', 'a seascape photograph', 'a photo of a beach by the ocean', 'a photo of a ship on the sea'],
    'forest': ['a photo of a forest', 'a photo of woodland'],
    'night': ['a photo of a night scene', 'a photo of a city at night'],
}

NEGATIVES = {
    'person': ['a photo without people', 'a landscape photo without people', 'a photo of a human statue', 'a photo of a doll'],
    'sea': ['a photo of a lake', 'a photo of a river', 'a photo of the sky', 'a photo of a swimming pool'],
}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--reference', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    reference = json.loads((args.reference / 'probe.json').read_text())
    if reference['model_id'] != MODEL_ID or reference['revision'] != REVISION:
        raise ValueError('reference checkpoint differs')
    torch.set_num_threads(2)
    model = CLIPModel.from_pretrained(MODEL_ID, revision=REVISION, local_files_only=True).eval()
    processor = CLIPProcessor.from_pretrained(MODEL_ID, revision=REVISION, local_files_only=True)
    concepts = []
    with torch.no_grad():
        for key, prompts in PROMPTS.items():
            vectors = model.get_text_features(**processor(text=prompts, padding=True, return_tensors='pt'))
            vectors = vectors / vectors.norm(dim=-1, keepdim=True)
            vector = vectors.mean(dim=0)
            vector = vector / vector.norm()
            concept = dict(key=key, prompts=prompts, embedding=vector.tolist())
            if key in NEGATIVES:
                vectors = model.get_text_features(**processor(text=NEGATIVES[key], padding=True, return_tensors='pt'))
                vectors = vectors / vectors.norm(dim=-1, keepdim=True)
                negative = vectors.mean(dim=0)
                negative = negative / negative.norm()
                concept['negative_embedding'] = negative.tolist()
            concepts.append(concept)
    args.output.mkdir(parents=True, exist_ok=True)
    reference['concepts'] = concepts
    reference['prompt_revision'] = 'raybend-tinyclip-v1-contrastive'
    reference['negative_prompts'] = NEGATIVES
    for name in ('image_encoder.onnx', 'parity-input.f32', 'parity-expected.f32'):
        shutil.copyfile(args.reference / name, args.output / name)
    (args.output / 'probe.json').write_text(json.dumps(reference, indent=2) + '\n')

if __name__ == '__main__':
    main()
