import test from 'node:test';import assert from 'node:assert/strict';
import {displayVariantKey,readDisplayVariantKey,displayedReference} from './display-variant.ts';
test('display keys preserve Unicode and snapshot identity, ordinary paths and malformed keys stay paths',()=>{
 const value={repositoryId:'中文库',reference:{assetId:42,variant:'issue:7'}};
 assert.deepEqual(readDisplayVariantKey(displayVariantKey(value)),value);
 for(const key of ['C:/照片/raw.nef','[bad','[]','["other",{}]','["raybend.display.v1",{"reference":{"assetId":-1}}]'])assert.equal(readDisplayVariantKey(key),null);
 assert.deepEqual(displayedReference(42,'latest',{issue:7}),{assetId:42,variant:'issue:7'});
 assert.deepEqual(displayedReference(42,'latest','sooc'),{assetId:42,variant:'sooc'});
 assert.deepEqual(displayedReference(42,'raw','latest'),{assetId:42,variant:'raw'});
});
