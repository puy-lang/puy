program StringListDelimited;

// The text forms of a TStringList (docs/spec/stdlib/System.Classes/TStringList.md): CommaText
// and DelimitedText both ways (quoting, spaces, empty strings, a delimiter at the end),
// StrictDelimiter, a Delimiter and QuoteChar of their own, and the name=value lines: Names,
// Values, ValueFromIndex, IndexOfName and NameValueSeparator (reading them; writing Values is in
// golden/runtime/stdlib_stringlist_values, as FPC keeps a line set to '').

uses System.SysUtils, System.Classes;

procedure Show(const What: string; L: TStrings);
var
  I: Integer;
begin
  Write(What, ' (', L.Count, '):');
  for I := 0 to L.Count - 1 do
    Write(' [', L[I], ']');
  WriteLn;
end;

procedure Parse(L: TStringList; const S: string);
begin
  L.DelimitedText := S;
  Show('<' + S + '>', L);
end;

var
  L: TStringList;
begin
  L := TStringList.Create;
  L.Add('plain');
  L.Add('two words');
  L.Add('a,b');
  L.Add('say "hi"');
  L.Add('');
  L.Add('tab'#9'here');
  WriteLn(L.CommaText);
  WriteLn(L.DelimitedText);
  L.StrictDelimiter := True;
  WriteLn(L.DelimitedText);
  L.StrictDelimiter := False;
  L.Clear;
  L.Add('');
  WriteLn(L.CommaText);
  L.Clear;
  WriteLn('[', L.CommaText, ']');

  Parse(L, 'a,b,c');
  Parse(L, 'a b  c');
  Parse(L, ' a , b ');
  Parse(L, '"x, y",z');
  Parse(L, '"say ""hi""",q');
  Parse(L, 'a,,b');
  Parse(L, 'a,');
  Parse(L, ',a');
  Parse(L, 'a ,');
  Parse(L, '""');
  Parse(L, '');
  Parse(L, '   ');
  Parse(L, '"open');
  L.StrictDelimiter := True;
  Parse(L, ' a b ,c ');
  Parse(L, 'a,,b,');
  Parse(L, '"q,r",s');
  L.StrictDelimiter := False;

  L.Delimiter := ';';
  L.QuoteChar := '''';
  Parse(L, 'a;b c;''d;e''');
  L.Add('x;y');
  WriteLn(L.DelimitedText);
  WriteLn(L.CommaText);
  L.Delimiter := ',';
  L.QuoteChar := '"';

  L.Text := 'host=example.org'#10'Port=80'#10'novalue'#10'empty='#10'=nameless'#10'a=b=c';
  WriteLn(L.Names[0], ' ', L.Names[1], ' [', L.Names[2], '] [', L.Names[4], '] ', L.Names[5]);
  WriteLn(L.Values['HOST'], ' ', L.Values['port'], ' [', L.Values['empty'], '] [',
    L.Values['missing'], '] ', L.Values['a']);
  WriteLn(L.ValueFromIndex[1], ' [', L.ValueFromIndex[3], '] ', L.ValueFromIndex[5]);
  WriteLn(L.IndexOfName('port'), ' ', L.IndexOfName('novalue'), ' ', L.IndexOfName(''));
  L.NameValueSeparator := ':';
  L.Clear;
  L.Add('key:value');
  L.Add('k=v');
  WriteLn(L.Names[0], ' ', L.Values['key'], ' [', L.Values['k'], ']');
  L.Free;
end.
