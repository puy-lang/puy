program StdioShadow;

// A declaration of the program hides `Output` and `sLineBreak` of `System` (semantics.md 4.1);
// `System.Output` still names the file.

var
  Output: Integer = 3;
  sLineBreak: string = '|';
begin
  WriteLn(Output, sLineBreak);
  WriteLn(System.Output, 'x', System.sLineBreak = #10);
end.
