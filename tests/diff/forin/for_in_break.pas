program ForInBreak;

// `Break` and `Continue` in a `for..in` body, including nested loops.

type
  TInts = array of Integer;

var
  D: TInts;
  E, F, I: Integer;
  C: Char;

begin
  SetLength(D, 6);
  for I := 0 to 5 do
    D[I] := I;

  Write('continue:');
  for E in D do
  begin
    if E mod 2 = 0 then
      Continue;
    Write(' ', E);
  end;
  WriteLn;

  Write('break:');
  for E in D do
  begin
    if E = 3 then
      Break;
    Write(' ', E);
  end;
  WriteLn;

  Write('nested:');
  for E in D do
  begin
    if E > 2 then
      Break;
    for F in D do
    begin
      if F > E then
        Break;
      Write(' ', E, F);
    end;
  end;
  WriteLn;

  Write('string:');
  for C in 'a-b-c' do
  begin
    if C = '-' then
      Continue;
    Write(' ', C);
  end;
  WriteLn;
end.
