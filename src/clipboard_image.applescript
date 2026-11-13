use framework "AppKit"
on run argv
if (count of argv) > 0 then
    set p to current application's NSPasteboard's pasteboardWithName:(item 1 of argv)
else
    set p to current application's NSPasteboard's generalPasteboard()
end if
set d to p's dataForType:"public.png"
if d is missing value then
    set imgs to current application's NSImage's alloc()'s initWithPasteboard:p
    if imgs is missing value then return ""
    set r to current application's NSBitmapImageRep's imageRepWithData:(imgs's TIFFRepresentation())
    set d to r's representationUsingType:4 |properties|:(current application's NSDictionary's dictionary())
end if
if d is missing value then return ""
return (d's base64EncodedStringWithOptions:0) as text
end run
