program DumpIr;

// Shape of the ir dump (bytecode.md 5.1) for the M1 constructs: branches, loops, a `for` whose
// loop variable is a global, calls with value / var / out arguments, and a write to a string element

var
  G: Integer;
  S: string;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

procedure Fill(var Total: Integer; out Count: Byte; const Tag: string);
begin
  Total := Total + Length(Tag);
  Count := 1;
end;

var
  N: Integer;
  C: Byte;
begin
  S := 'abc';
  S[2] := 'x';
  N := Twice(3);
  Fill(N, C, S);
  if (N > 5) and (C = 1) then
    WriteLn('big ', N)
  else
    WriteLn('small');
  while N > 0 do
    Dec(N, 4);
  repeat
    Inc(N);
    if N = 2 then
      Continue;
  until N >= 3;
  for G := 1 to 3 do
  begin
    if G = 2 then
      Break;
    Write(G);
  end;
  WriteLn;
  WriteLn(S, ' ', N, ' ', C);
end.
