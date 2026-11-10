program Strings;

uses
  System.SysUtils;

// String handling: splitting, searching, case conversion and Unicode.

function SplitWords(const Text: string): TArray<string>;
var
  Current: string;
begin
  Result := [];
  Current := '';
  for var C in Text do
    if C = ' ' then
    begin
      if Current <> '' then
        Result := Result + [Current];
      Current := '';
    end
    else
      Current := Current + C;
  if Current <> '' then
    Result := Result + [Current];
end;

function Capitalize(const S: string): string;
begin
  Result := UpperCase(Copy(S, 1, 1)) + LowerCase(Copy(S, 2, Length(S) - 1));
end;

const
  Text = '  the quick  brown fox jumps over the lazy dog  ';

var
  Words: TArray<string>;
  Longest: string;

begin
  Words := SplitWords(Text);
  WriteLn('Words: ', Length(Words));

  Longest := '';
  for var W in Words do
  begin
    Write(Capitalize(W), ' ');
    if Length(W) > Length(Longest) then
      Longest := W;
  end;
  WriteLn;
  WriteLn('Longest: ', QuotedStr(Longest));

  WriteLn('Trimmed: [', Trim(Text), ']');
  WriteLn('Position of "fox": ', Pos('fox', Text));
  WriteLn(StringReplace(Trim(Text), 'the', 'a', [TReplaceFlag.rfReplaceAll]));

  // string is UTF-16; Length counts code units.
  WriteLn('Length(''日本語'') = ', Length('日本語'));
  WriteLn(StringOfChar('=', 20));
end.
