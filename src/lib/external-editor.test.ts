import test from 'node:test';import assert from 'node:assert/strict';
import {readExternalPreferences,externalRunning,emptyExternalTask,applicationKey} from './external-editor.ts';
test('versioned editor settings reject corrupted/high-version data and deduplicate Unicode Windows paths',()=>{
 assert.equal(readExternalPreferences(null).applications.length,0);
 const apps=[{name:'Affinity',path:'C:\\应用\\Photo.exe'},{name:'duplicate',path:'c:/应用/photo.EXE'},{name:'',path:'x'}];
 const prefs=readExternalPreferences(JSON.stringify({version:1,applications:apps,directory:'D:/照片',lastApplication:apps[0].path}));assert.equal(prefs.applications.length,1);assert.equal(applicationKey(apps[0].path),applicationKey(apps[1].path));
 for(const raw of ['{','null',JSON.stringify({...prefs,version:2}),JSON.stringify({...prefs,applications:'bad'}),JSON.stringify({...prefs,directory:'bad\0'})])assert.throws(()=>readExternalPreferences(raw));
});
test('only active phases protect the background task',()=>{for(const status of ['preparing','rendering','writing','opening'])assert(externalRunning({...emptyExternalTask(),status}));for(const status of ['idle','done','failed','cancelled'])assert(!externalRunning({...emptyExternalTask(),status}));});

test('POSIX adapter paths retain case and literal backslashes',()=>{assert.notEqual(applicationKey('/Apps/Editor'),applicationKey('/apps/editor'));assert.equal(applicationKey('/apps/a\\b'),'/apps/a\\b');assert.equal(applicationKey('\\\\Server\\Apps\\Photo.exe'),'//server/apps/photo.exe');});
