program StringListTrailingDelimiter;

// A Delimiter at the end of DelimitedText (docs/spec/stdlib/System.Classes/TStringList.md 2.3):
// it adds an empty string only when it is the last character, as Delphi XE4 does; spaces after
// it do not (FPC 3.2.2 adds one there too, so this is not a diff/ case).

uses System.SysUtils, System.Classes;

procedure Parse(L: TStringList; const S: string);
begin
  L.DelimitedText := S;
  Write('<', S, '> (', L.Count, '):');
  for var I := 0 to L.Count - 1 do
    Write(' [', L[I], ']');
  WriteLn;
end;

begin
  var L := TStringList.Create;
  Parse(L, 'a,');
  Parse(L, 'a, ');
  Parse(L, 'a ,');
  Parse(L, 'a,  ');
  L.StrictDelimiter := True;
  Parse(L, 'a, ');
  L.Free;
end.
