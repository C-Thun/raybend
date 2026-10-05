---
license: mit
datasets:
- laion/laion400m
pipeline_tag: zero-shot-image-classification
tags:
- tinyclip
---
# TinyCLIP: CLIP Distillation via Affinity Mimicking and Weight Inheritance

**[ICCV 2023]** - [TinyCLIP: CLIP Distillation via Affinity Mimicking and Weight Inheritance](https://openaccess.thecvf.com/content/ICCV2023/html/Wu_TinyCLIP_CLIP_Distillation_via_Affinity_Mimicking_and_Weight_Inheritance_ICCV_2023_paper.html)

**TinyCLIP** is a novel **cross-modal distillation** method for large-scale language-image pre-trained models. The method introduces two core techniques: **affinity mimicking** and **weight inheritance**. This work unleashes the capacity of small CLIP models, fully leveraging large-scale models as well as pre-training data and striking the best trade-off between speed and accuracy.

<p align="center">
  <img src="./figure/TinyCLIP.jpg" width="1000">
</p>


## Use with Transformers

```python
from PIL import Image
import requests

from transformers import CLIPProcessor, CLIPModel

model = CLIPModel.from_pretrained("wkcn/TinyCLIP-ViT-40M-32-Text-19M-LAION400M")
processor = CLIPProcessor.from_pretrained("wkcn/TinyCLIP-ViT-40M-32-Text-19M-LAION400M")

url = "http://images.cocodataset.org/val2017/000000039769.jpg"
image = Image.open(requests.get(url, stream=True).raw)

inputs = processor(text=["a photo of a cat", "a photo of a dog"], images=image, return_tensors="pt", padding=True)

outputs = model(**inputs)
logits_per_image = outputs.logits_per_image # this is the image-text similarity score
probs = logits_per_image.softmax(dim=1) # we can take the softmax to get the label probabilities
```

## Highlights
<p align="center">
  <img src="./figure/fig1.jpg" width="500">
</p>

* TinyCLIP ViT-45M/32 uses only **half parameters** of ViT-B/32 to achieves **comparable zero-shot performance**.
* TinyCLIP ResNet-19M reduces the parameters by **50\%** while getting **2x** inference speedup, and obtains **56.4\%** accuracy on ImageNet.

## Model Zoo
| Model              | Weight inheritance | Pretrain      | IN-1K Acc@1(%) | MACs(G) | Throughput(pairs/s) | Link |
|--------------------|--------------------|---------------|----------------|---------|---------------------|------|
[TinyCLIP ViT-39M/16 Text-19M](./src/open_clip/model_configs/TinyCLIP-ViT-39M-16-Text-19M.json) | manual | YFCC-15M | 63.5  | 9.5 | 1,469 | [Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-ViT-39M-16-Text-19M-YFCC15M.pt)
[TinyCLIP ViT-8M/16 Text-3M](./src/open_clip/model_configs/TinyCLIP-ViT-8M-16-Text-3M.json) | manual | YFCC-15M | 41.1 | 2.0 | 4,150 | [Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-ViT-8M-16-Text-3M-YFCC15M.pt)
[TinyCLIP ResNet-30M Text-29M](./src/open_clip/model_configs/TinyCLIP-ResNet-30M-Text-29M.json) | manual | LAION-400M | 59.1  | 6.9 | 1,811 | [Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-ResNet-30M-Text-29M-LAION400M.pt)
[TinyCLIP ResNet-19M Text-19M](./src/open_clip/model_configs/TinyCLIP-ResNet-19M-Text-19M.json) | manual | LAION-400M | 56.4  | 4.4 | 3,024| [Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-ResNet-19M-Text-19M-LAION400M.pt)
[TinyCLIP ViT-61M/32 Text-29M](./src/open_clip/model_configs/TinyCLIP-ViT-61M-32-Text-29M.json) | manual | LAION-400M | 62.4  | 5.3 | 3,191|[Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-ViT-61M-32-Text-29M-LAION400M.pt)
[TinyCLIP ViT-40M/32 Text-19M](./src/open_clip/model_configs/TinyCLIP-ViT-40M-32-Text-19M.json) | manual | LAION-400M | 59.8  | 3.5 | 4,641|[Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-ViT-40M-32-Text-19M-LAION400M.pt)
TinyCLIP ViT-63M/32 Text-31M | auto | LAION-400M | 63.9  | 5.6 | 2,905|[Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-auto-ViT-63M-32-Text-31M-LAION400M.pt)
TinyCLIP ViT-45M/32 Text-18M | auto | LAION-400M | 61.4  | 3.7 | 3,682|[Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-auto-ViT-45M-32-Text-18M-LAION400M.pt)
TinyCLIP ViT-22M/32 Text-10M | auto | LAION-400M | 53.7  | 1.9 | 5,504|[Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-auto-ViT-22M-32-Text-10M-LAION400M.pt)
TinyCLIP ViT-63M/32 Text-31M | auto | LAION+YFCC-400M | 64.5 | 5.6| 2,909 | [Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-auto-ViT-63M-32-Text-31M-LAIONYFCC400M.pt)
TinyCLIP ViT-45M/32 Text-18M | auto | LAION+YFCC-400M | 62.7  | 1.9 | 3,685 | [Model](https://github.com/wkcn/TinyCLIP-model-zoo/releases/download/checkpoints/TinyCLIP-auto-ViT-45M-32-Text-18M-LAIONYFCC400M.pt)

Note: The configs of models with auto inheritance are generated automatically.

## Official PyTorch Implementation
https://github.com/microsoft/Cream/tree/main/TinyCLIP

## Citation
If this repo is helpful for you, please consider to cite it. :mega: Thank you! :)

```bibtex
@InProceedings{tinyclip,
    title     = {TinyCLIP: CLIP Distillation via Affinity Mimicking and Weight Inheritance},
    author    = {Wu, Kan and Peng, Houwen and Zhou, Zhenghong and Xiao, Bin and Liu, Mengchen and Yuan, Lu and Xuan, Hong and Valenzuela, Michael and Chen, Xi (Stephen) and Wang, Xinggang and Chao, Hongyang and Hu, Han},
    booktitle = {Proceedings of the IEEE/CVF International Conference on Computer Vision (ICCV)},
    month     = {October},
    year      = {2023},
    pages     = {21970-21980}
}
```

## Acknowledge

Our code is based on [CLIP](https://github.com/openai/CLIP), [OpenCLIP](https://github.com/mlfoundations/open_clip), [CoFi](https://github.com/princeton-nlp/CoFiPruning) and [PyTorch](https://github.com/pytorch/pytorch). Thank contributors for their awesome contribution!

## License
- [License](https://github.com/microsoft/Cream/blob/main/TinyCLIP/LICENSE)