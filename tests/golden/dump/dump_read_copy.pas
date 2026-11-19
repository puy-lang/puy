program DumpReadCopy;

// The counted copy of a global (or an element or an instance's field) read by a string
// comparison, by `SameText` (`intrinsic_pair`), as the string of `G[One]`, as the selector of
// `case` and as an operand of `Write` is let go of when the statement ends, before the body of
// `if` / `while` for a condition (lifetime.md 3.3, 3.4), so the next `G := G + 'x'` appends in
// place; the string of `G[1]` is read without counting (its index runs no code); a copy of an
// object stays to the end of the routine (ir).

uses
  System.SysUtils;

type
  TBox = class
  public
    S: string;
  end;

function One: Integer;
begin
  Result := 1;
end;

var
  G, H: string;
  SA: array of string;
  B: array of TObject;
  Box: TBox;
  N, I: Integer;
begin
  N := 0;
  G := 'a';
  for I := 1 to 3 do
  begin
    if G = 'abc' then
      N := N + 1;
    if SameText(G, 'x') then
      N := N + 1;
    if UpCase(G[1]) = 'X' then
      N := N + 1;
    if G[One] = 'x' then
      N := N + 1;
    Write('[', G);
    G := G + 'x';
  end;
  WriteLn;
  while G <> 'axxxxx' do
    G := G + 'x';
  SetLength(SA, 1);
  if SA[0] = '' then
    N := N + 1;
  Box := TBox.Create;
  if Box.S = '' then
    N := N + 1;
  Box.Free;
  case G of
    'axxxxx': N := N + 1;
  end;
  H := G;
  SetLength(B, 1);
  if B[0] = nil then
    N := N + 1;
  WriteLn(N, ' ', G, ' ', H);
end.
