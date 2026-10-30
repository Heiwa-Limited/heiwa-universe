// Fixed JXA program. User data travels on stdin and is never evaluated as code.
ObjC.import('Foundation');
function run() {
    let createdId = null;
    try {
        const data = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
        const request = JSON.parse(ObjC.unwrap($.NSString.alloc.initWithDataEncoding(data, $.NSUTF8StringEncoding)));
        const app = Application('Notes');
        const args = request.arguments;
        if (request.operation === 'notes.folders') {
            const folders = app.folders();
            return JSON.stringify({folders: folders.slice(0, 50).map(f => ({id:f.id(), name:f.name()})), truncated:folders.length>50});
        }
        if (request.operation === 'notes.search') {
            const matches = app.notes.whose({name:{_contains:args.query}})();
            return JSON.stringify({notes:matches.slice(0,30).map(n=>({id:n.id(),title:n.name()})),truncated:matches.length>30});
        }
        if (request.operation === 'notes.read') {
            const note = app.notes.byId(args.id);
            if (!note.exists()) throw new Error('Note ID unavailable');
            const text = note.plaintext();
            return JSON.stringify({id:note.id(),title:note.name(),text:Array.from(text).slice(0,32000).join(''),truncated:Array.from(text).length>32000});
        }
        if (request.operation === 'notes.create') {
            if (!args.title.trim() || args.title.length>200 || args.body.length>32000) throw new Error('Invalid note size');
            const folder = app.folders.byId(args.folder_id);
            if (!folder.exists()) throw new Error('Folder ID unavailable');
            const escape = s => s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
            const html = '<h1>'+escape(args.title)+'</h1><div>'+escape(args.body).replace(/\n/g,'<br>')+'</div>';
            // Notes derives its name from the first heading. Setting both
            // name and a heading inserts the title twice in the saved note.
            const note = app.Note({body:html});
            folder.notes.push(note);
            createdId = note.id();
            const saved = app.notes.byId(createdId);
            const text = saved.plaintext();
            if (saved.name() !== args.title || !text.replace(/\r\n?/g,'\n').includes(args.body.replace(/\r\n?/g,'\n'))) throw new Error('Saved note could not be verified');
            return JSON.stringify({id:saved.id(),title:saved.name(),folder_id:folder.id(),verified:true,text:Array.from(text).slice(0,32000).join('')});
        }
        throw new Error('Unsupported Notes operation');
    } catch (error) {
        // No note contents or native exception details in failures.
        return JSON.stringify({error:createdId ? 'Note creation outcome is uncertain. Search for the created note before retrying.' : 'Notes action failed; check the selected ID and Heiwa Automation permission for Notes.',created_id:createdId});
    }
}
