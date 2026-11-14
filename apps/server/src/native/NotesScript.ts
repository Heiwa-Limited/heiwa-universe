/** Static JXA program; request data is decoded from stdin and never evaluated. */
export const NOTES_SCRIPT = String.raw`
ObjC.import('Foundation');
function run() {
  let writing = false;
  try {
    const bytes = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
    const text = ObjC.unwrap($.NSString.alloc.initWithDataEncoding(bytes, $.NSUTF8StringEncoding));
    if (text.length > 1048576) throw 'invalid';
    const q = JSON.parse(text);
    const app = Application('com.apple.Notes');
    const folders = app.folders();
    const folder = id => {
      const rows = folders.filter(f => f.id() === id);
      if (rows.length !== 1) throw 'missing';
      return rows[0];
    };
    const record = note => {
      const title = note.name() || '';
      let locked = false; try { locked = note.passwordProtected(); } catch {}
      const bodyHtml = locked ? '' : note.body() || '';
      let body = locked ? '' : note.plaintext() || '';
      if (body.startsWith(title + '\n')) body = body.slice(title.length + 1).replace(/^\n/, '');
      return { app:'notes', id:note.id(), resourceId:note.container().id(), title:title.slice(0,16000),
        body:body.slice(0,16000), bodyHtml:bodyHtml.slice(0,16000), start:null, end:null, completed:false,
        modifiedAt:note.modificationDate().toISOString(), recurring:false, hasAttendees:false,
        contentTruncated:title.length > 16000 || body.length > 16000 || bodyHtml.length > 16000,
        locked, hasAttachments:!locked && note.attachments().length > 0 };
    };
    if (q.operation === 'inventory') return JSON.stringify({resources:folders.map(f => ({app:'notes',id:f.id(),title:f.name(),source:f.container().name(),writable:!f.shared() && f.name() !== 'Recently Deleted'}))});
    if (!Array.isArray(q.resourceIds) || q.resourceIds.length < 1 || q.resourceIds.length > 100) throw 'invalid';
    const selected = q.resourceIds.map(folder);
    if (q.operation === 'inspect') {
      const matches = folder(q.resourceIds[0]).notes().filter(n => n.id() === q.id);
      if (matches.length !== 1) throw 'missing';
      return JSON.stringify({record:record(matches[0])});
    }
    if (q.operation === 'read') {
      if (!Number.isInteger(q.limit) || q.limit < 1 || q.limit > 100) throw 'invalid';
      let notes = [];
      for (const f of selected) {
        notes = notes.concat(f.notes());
        if (notes.length > q.limit) break;
      }
      return JSON.stringify({records:notes.slice(0,q.limit).map(record),truncated:notes.length > q.limit});
    }
    if (!q.resourceIds.includes(q.resourceId)) throw 'invalid';
    const targetFolder = folder(q.resourceId);
    if (targetFolder.shared() || targetFolder.name() === 'Recently Deleted') throw 'invalid';
    const escape = s => String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/\n/g,'<br>');
    const content = (title, body) => '<div>' + escape(title) + '</div><div><br></div><div>' + escape(body) + '</div>';
    if (q.operation === 'create') {
      if (typeof q.title !== 'string' || !q.title.trim() || q.title.length > 800 || typeof q.body !== 'string') throw 'invalid';
      const note = app.Note({body:content(q.title,q.body)});
      writing = true;
      targetFolder.notes.push(note);
      return JSON.stringify({record:record(note)});
    }
    const matches = targetFolder.notes().filter(n => n.id() === q.id);
    if (matches.length !== 1 || !q.expected) throw 'missing';
    const note = matches[0], actual = record(note);
    for (const key of Object.keys(actual)) if (actual[key] !== q.expected[key]) throw 'stale';
    if (actual.locked || note.shared()) throw 'invalid';
    if (q.operation === 'trash') {
      writing = true;
      app.delete(note);
      if (targetFolder.notes().some(n => n.id() === q.id)) throw 'uncertain';
      return JSON.stringify({deleted:true});
    }
    if (!['update','append'].includes(q.operation) || actual.contentTruncated || actual.hasAttachments || note.shared()) throw 'invalid';
    writing = true;
    if (q.operation === 'append') {
      if (typeof q.body !== 'string' || !q.body.trim()) throw 'invalid';
      note.body = actual.bodyHtml + '<div>' + escape(q.body) + '</div>';
      return JSON.stringify({record:record(note)});
    }
    if (q.body !== undefined) note.body = content(q.title === undefined ? actual.title : q.title, q.body);
    if (q.title !== undefined) note.name = q.title;
    return JSON.stringify({record:record(note)});
  } catch (e) {
    const code = writing ? 'uncertain' : ['invalid','missing','stale','uncertain'].includes(String(e)) ? String(e) : 'permission';
    return JSON.stringify({error:code});
  }
}
`;
