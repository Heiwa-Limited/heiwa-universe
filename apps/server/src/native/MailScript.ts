/**
 * Static JXA program; request data is decoded from stdin and never evaluated.
 * Listing comes from Mail's index. This script only reads content and performs the
 * three supported writes: read state, saving a draft, and moving a message to Trash.
 * It never sends mail.
 */
export const MAIL_SCRIPT = String.raw`
ObjC.import('Foundation');
function run() {
  let writing = false;
  try {
    const bytes = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
    const text = ObjC.unwrap($.NSString.alloc.initWithDataEncoding(bytes, $.NSUTF8StringEncoding));
    if (text.length > 1048576) throw 'invalid';
    const q = JSON.parse(text);
    const app = Application('com.apple.Mail');
    if (q.operation === 'accounts') {
      const ids = app.accounts.id(), names = app.accounts.name();
      return JSON.stringify({accounts:ids.map((id, i) => ({id, name:String(names[i] || '')}))});
    }
    if (typeof q.accountId !== 'string' || !q.accountId) throw 'invalid';
    const account = app.accounts.byId(q.accountId);
    try { account.name(); } catch { throw 'missing'; }
    const exists = mb => { try { mb.name(); return true; } catch { return false; } };
    const mailbox = path => {
      if (typeof path !== 'string' || !path) throw 'invalid';
      const leaf = path.split('/').pop();
      for (const name of [path, path.replace(/^\[Gmail\]\//, ''), leaf]) {
        const mb = account.mailboxes.byName(name);
        if (exists(mb)) return mb;
      }
      throw 'missing';
    };
    const find = (mb, id) => {
      if (typeof id !== 'string' || !/^\d{1,18}$/.test(id)) throw 'invalid';
      const rows = mb.messages.whose({id:Number(id)})();
      if (rows.length !== 1) throw 'missing';
      return rows[0];
    };
    const clip = s => String(s || '').slice(0, 16000);
    if (q.operation === 'content') {
      if (!Array.isArray(q.ids) || q.ids.length < 1 || q.ids.length > 5) throw 'invalid';
      const mb = mailbox(q.path);
      return JSON.stringify({contents:q.ids.map(id => ({id, content:clip(find(mb, id).content())}))});
    }
    if (q.operation === 'complete') {
      if (typeof q.completed !== 'boolean') throw 'invalid';
      const message = find(mailbox(q.path), q.id);
      writing = true;
      message.readStatus = q.completed;
      return JSON.stringify({completed:message.readStatus()});
    }
    if (q.operation === 'trash') {
      const mb = mailbox(q.path);
      const message = find(mb, q.id);
      writing = true;
      // Mail moves the message to the account's Trash, where it stays recoverable.
      app.delete(message);
      // IMAP moves complete asynchronously; wait briefly for the source mailbox to drop it.
      for (let attempt = 0; attempt < 20; attempt++) {
        if (mb.messages.whose({id:Number(q.id)})().length === 0) return JSON.stringify({deleted:true});
        delay(0.25);
      }
      throw 'uncertain';
    }
    if (q.operation === 'create') {
      if (typeof q.title !== 'string' || !q.title.trim() || q.title.length > 800 || typeof q.body !== 'string') throw 'invalid';
      const recipients = q.recipients === undefined ? [] : q.recipients;
      if (!Array.isArray(recipients) || recipients.length > 20 || recipients.some(r => typeof r !== 'string')) throw 'invalid';
      const addresses = account.emailAddresses();
      if (addresses.length < 1) throw 'invalid';
      // A unique marker would leak into the draft; the subject plus newest id identifies it instead.
      // Each account chooses its own drafts mailbox; Mail's unified Drafts covers all of them.
      const drafts = app.draftsMailbox;
      const before = drafts.messages.whose({subject:q.title}).id();
      const draft = app.OutgoingMessage({subject:q.title, content:q.body, visible:false});
      writing = true;
      app.outgoingMessages.push(draft);
      draft.sender = addresses[0];
      for (const address of recipients) draft.toRecipients.push(app.Recipient({address}));
      app.save(draft);
      try { app.close(draft, {saving:'no'}); } catch {}
      for (let attempt = 0; attempt < 20; attempt++) {
        const ids = drafts.messages.whose({subject:q.title}).id().filter(id => !before.includes(id));
        if (ids.length === 1) {
          const saved = find(drafts, String(ids[0]));
          if (saved.mailbox().account().id() !== q.accountId) throw 'uncertain';
          return JSON.stringify({id:String(ids[0]), subject:clip(saved.subject()), content:clip(saved.content())});
        }
        if (ids.length > 1) throw 'uncertain';
        delay(0.25);
      }
      throw 'uncertain';
    }
    throw 'invalid';
  } catch (e) {
    const code = writing ? 'uncertain' : ['invalid','missing','stale','uncertain'].includes(String(e)) ? String(e) : 'permission';
    return JSON.stringify({error:code});
  }
}
`;
