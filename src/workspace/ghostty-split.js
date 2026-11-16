// Copyright (c) 2026 Govijr. MIT license; see third_party/terminal-phone/LICENSE.
// Adapted from terminal-phone TerminalSplit.swift; arguments remain separate Apple Event values.
ObjC.import("Foundation");
function run(argv) {
  const [tty, directory, direction, command, grow = "0"] = argv;
  const D = $.NSAppleEventDescriptor;
  const code = (t) => ((t.charCodeAt(0) << 24) | (t.charCodeAt(1) << 16) | (t.charCodeAt(2) << 8) | t.charCodeAt(3)) >>> 0;
  const type = (t) => D.descriptorWithTypeCode(code(t));
  const enumerated = (t) => D.descriptorWithEnumCode(code(t));
  const text = (t) => D.descriptorWithString(t);
  const all = D.descriptorWithDescriptorTypeData(code("abso"), $(" lla").dataUsingEncoding($.NSMacOSRomanStringEncoding));
  function specifier(wanted, from, form, key) {
    const record = D.recordDescriptor;
    record.setDescriptorForKeyword(type(wanted), code("want"));
    record.setDescriptorForKeyword(from, code("from"));
    record.setDescriptorForKeyword(enumerated(form), code("form"));
    record.setDescriptorForKeyword(key, code("seld"));
    return record.coerceToDescriptorType(code("obj "));
  }
  const every = (kind, from) => specifier(kind, from, "indx", all);
  const property = (name, of) => specifier("prop", of, "prop", type(name));
  const ghostty = D.descriptorWithBundleIdentifier("com.mitchellh.ghostty");
  function send(eventClass, eventId, params) {
    const event = D.appleEventWithEventClassEventIDTargetDescriptorReturnIDTransactionID(code(eventClass), code(eventId), ghostty, -1, 0);
    for (const key of Object.keys(params)) event.setParamDescriptorForKeyword(params[key], code(key));
    const failure = Ref();
    const reply = event.sendEventWithOptionsTimeoutError(3, 10, failure);
    if (reply.isNil()) throw new Error("Ghostty did not answer.");
    const number = reply.paramDescriptorForKeyword(code("errn"));
    if (!number.isNil()) throw new Error("Ghostty refused the request (" + number.int32Value + ").");
    return reply.paramDescriptorForKeyword(code("----"));
  }
  function flatten(descriptor, out) {
    if (descriptor.isNil()) { out.push(""); return out; }
    if (descriptor.descriptorType === code("list")) {
      for (let i = 1; i <= descriptor.numberOfItems; i++) flatten(descriptor.descriptorAtIndex(i), out);
      return out;
    }
    const value = descriptor.stringValue;
    out.push(value.isNil() ? "" : ObjC.unwrap(value));
    return out;
  }
  const terminals = every("Gtrm", every("Gtab", every("Gwnd", D.nullDescriptor)));
  const read = (name) => { try { return flatten(send("core", "getd", { "----": property(name, terminals) }), []); } catch (e) { return []; } };
  const ids = read("ID  "), ttys = read("Gtty"), directories = read("Gwdr");
  let index = tty ? ttys.indexOf(tty) : -1;
  if (index < 0 && tty) index = ids.indexOf(tty);
  if (index < 0 && direction !== "focus" && direction !== "close") {
    const here = directories.map((d, i) => (d === directory ? i : -1)).filter((i) => i >= 0);
    if (here.length === 1) index = here[0];
  }
  if (index < 0 || !ids[index]) throw new Error("Could not find this pane in Ghostty. Run the command from the pane you want to split.");
  const owner = specifier("Gtrm", D.nullDescriptor, "ID  ", text(ids[index]));
  if (direction === "focus") {
    send("Ghst", "Fcus", {"----": owner});
    return ids[index];
  }
  if (direction === "close") {
    send("Ghst", "Clos", {"----": owner});
    return ids[index];
  }
  const configuration = D.recordDescriptor;
  configuration.setDescriptorForKeyword(text(directory), code("GScD"));
  configuration.setDescriptorForKeyword(text(command), code("GScC"));
  configuration.setDescriptorForKeyword(D.descriptorWithBoolean(false), code("GScW"));
  const created = send("Ghst", "Splt", { "----": owner, GSpd: enumerated(direction), GSpS: configuration });
  const pane = flatten(send("core", "getd", {"----": property("ID  ", created)}), [])[0];
  const pixels = Math.max(0, Math.min(200, Number(grow) || 0));
  if (pixels) send("Ghst", "PfAc", {"----": text("resize_split:right," + pixels), GonT: owner});
  return pane;
}
