"""Reproduce the bounded public cohort; no dataset/image is bundled with RayBend.
Download the three metadata files listed in docs/ai/2026-10-04/README.md first.
All unlabelled concepts remain unknown, not negatives. Image attribution is retained.
"""
import argparse
import concurrent.futures
import csv
import hashlib
import json
import pathlib
import urllib.request

CLASSES = {
    'person': '/m/01g317', 'bird': '/m/015p6', 'car': '/m/0k4j',
    'mountain': '/m/09d_r', 'sea': '/m/06npx', 'forest': '/m/02zr8', 'night': '/m/01d74z',
}
SEED = 'raybend-ai-w1-2026-10-04'
METADATA_SHA256 = {
    'labels.csv': 'c2bc8b312ee7ed01ac4c58a62c2432c7f6b83b320874cbb0644e2f51aed3faad',
    'images.csv': 'ed93a0e121fe345effdfc7359b848dbc64a1ff6778c8c73563157cb500b33a17',
}

def split(row):
    group = row['AuthorProfileURL'] or row['Author'] or row['OriginalMD5']
    return 'calibration' if int(hashlib.sha256(group.encode()).hexdigest()[:8], 16) % 2 == 0 else 'validation'

def select(known, metadata):
    selected = set()
    for concept in CLASSES:
        for part in ('calibration', 'validation'):
            for positive, limit in ((1, 50), (0, 80)):
                ids = [i for i, labels in known.items() if labels.get(concept) == positive and split(metadata[i]) == part]
                ids.sort(key=lambda i: hashlib.sha256((SEED + i).encode()).hexdigest())
                selected.update(ids[:limit])
    return sorted(selected)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--directory', type=pathlib.Path, required=True)
    parser.add_argument('--download', action='store_true', help='Fetch at most 8 MiB per selected image, five concurrent requests.')
    args = parser.parse_args()
    root = args.directory
    for name, expected in METADATA_SHA256.items():
        if hashlib.sha256((root/name).read_bytes()).hexdigest() != expected:
            raise ValueError('metadata hash mismatch: ' + name)
    label_keys = {v: k for k, v in CLASSES.items()}
    known = {}
    with (root/'labels.csv').open() as stream:
        for row in csv.DictReader(stream):
            if row['LabelName'] in label_keys:
                value = int(row['Confidence'])
                if value not in (0, 1):
                    raise ValueError('expected human verified 0/1 label')
                known.setdefault(row['ImageID'], {})[label_keys[row['LabelName']]] = value
    with (root/'images.csv').open() as stream:
        metadata = {r['ImageID']: r for r in csv.DictReader(stream)}
    images = root/'images'
    images.mkdir(exist_ok=True)
    def fetch(image_id):
        row = metadata[image_id]
        path = images/(image_id+'.jpg')
        url = 'https://open-images-dataset.s3.amazonaws.com/validation/'+image_id+'.jpg'
        record = dict(id=image_id, path=str(path.resolve()), split=split(row), labels=known[image_id],
                      source=url, license=row['License'], author=row['Author'],
                      author_profile=row['AuthorProfileURL'], landing=row['OriginalLandingURL'])
        if args.download and not path.exists():
            try:
                with urllib.request.urlopen(url, timeout=20) as response:
                    data = response.read(8*1024*1024+1)
                if len(data) > 8*1024*1024:
                    raise ValueError('image exceeds 8 MiB experiment limit')
                path.write_bytes(data)
            except Exception as error:
                record.pop('path')
                record['download_error'] = str(error)
                return record
        if path.exists():
            record['sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
        else:
            record.pop('path')
            record['download_error'] = 'not downloaded; pass --download to fetch'
        return record
    with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
        rows = list(pool.map(fetch, select(known, metadata)))
    (root/'manifest.json').write_text(json.dumps(rows, ensure_ascii=False, indent=2)+'\n')
    print(json.dumps(dict(selected=len(rows), unavailable=sum('path' not in r for r in rows))))

if __name__ == '__main__':
    main()
