program StringListValues;

// Writing the name=value lines of a TStringList (docs/spec/stdlib/System.Classes/
// TStringList.md 2.4): Values[N] := '' removes the line, a new value rewrites the line with the
// name as written, ValueFromIndex keeps the name (`=` and the value on a line without one), and
// a sorted list refuses both rewrites.

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

begin
  var L := TStringList.Create;
  L.Text := 'host=example.org'#10'Port=80'#10'novalue';
  L.Values['port'] := '8080';
  L.Values['user'] := 'me';
  L.Values['host'] := '';
  L.Values['missing'] := '';
  Show('Values', L);
  L.ValueFromIndex[0] := '443';
  L.ValueFromIndex[1] := 'x';
  Show('ValueFromIndex', L);
  L.ValueFromIndex[2] := '';
  Show('ValueFromIndex removes', L);
  L.AddPair('k', 'v').AddPair('n', '1');
  Show('AddPair', L);
  L.Sorted := True;
  L.Values['zz'] := 'last';
  Show('sorted', L);
  try
    L.Values['k'] := 'w';
  except
    on E: EStringListError do
      WriteLn(E.Message);
  end;
  L.Free;
end.
