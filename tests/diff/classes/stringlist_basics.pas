program StringListBasics;

// TStringList as a plain list (docs/spec/stdlib/System.Classes/TStringList.md): Add, Insert,
// Delete, Exchange, Move, IndexOf (not case sensitive by default), Strings[] and Count, for..in,
// AddStrings and Assign, Text with the line breaks it splits at, and a LineBreak of its own.

uses System.SysUtils, System.Classes;

// The lines between brackets, so that empty lines show.
procedure Show(const What: string; L: TStrings);
var
  I: Integer;
begin
  Write(What, ' (', L.Count, '):');
  for I := 0 to L.Count - 1 do
    Write(' [', L[I], ']');
  WriteLn;
end;

// Text with every CR as \r and every LF as \n.
function Escaped(const S: string): string;
var
  I: Integer;
begin
  Result := '';
  for I := 1 to Length(S) do
    if S[I] = #13 then
      Result := Result + '\r'
    else if S[I] = #10 then
      Result := Result + '\n'
    else
      Result := Result + S[I];
end;

var
  L, M: TStringList;
  S: string;
begin
  L := TStringList.Create;
  WriteLn(L.Add('one'), ' ', L.Add('two'), ' ', L.Add('three'));
  L.Insert(0, 'zero');
  L.Insert(4, 'four');
  Show('added', L);
  L.Delete(2);
  Show('deleted', L);
  L.Exchange(0, 3);
  Show('exchanged', L);
  L.Move(0, 2);
  Show('moved', L);
  L[1] := 'ONE';
  Show('written', L);
  WriteLn(L.IndexOf('one'), ' ', L.IndexOf('Four'), ' ', L.IndexOf('five'));
  for S in L do
    Write(S, ';');
  WriteLn;

  M := TStringList.Create;
  M.Add('first');
  M.AddStrings(L);
  Show('AddStrings', M);
  M.Assign(L);
  Show('Assign', M);
  M.Clear;
  Show('Clear', M);

  L.Text := 'a'#13#10'b'#10'c'#13'd';
  Show('CR LF, LF, CR', L);
  L.Text := 'a'#13#10#13#10'b'#13#10;
  Show('empty line, break at the end', L);
  L.Text := '';
  Show('empty', L);
  L.Text := 'x';
  Show('no break', L);

  L.LineBreak := '|';
  L.Text := 'p|q||r';
  Show('LineBreak |', L);
  WriteLn(Escaped(L.Text));
  L.Free;
  M.Free;
end.
