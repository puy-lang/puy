program DumpRepeatCond;

// The temporaries of a `repeat ... until` condition are let go of on both paths: before going
// back for the next iteration and after the loop (lifetime.md 3.4), so the next
// `G := G + 'x'` appends in place. This holds for the counted copy of a global string and for a
// dynamic array holding no object; a condition with no such temporary goes straight back (ir).

type
  TInts = array of Integer;

function Make(N: Integer): TInts;
begin
  SetLength(Result, 1);
  Result[0] := N;
end;

var
  G: string;
  I: Integer;
begin
  G := 'a';
  repeat
    G := G + 'x';
  until G = 'axxx';
  repeat
    G := G + 'x';
    if G = 'axxxxx' then
      Continue;
  until Length(G) > 6;
  I := 0;
  repeat
    I := I + 1;
  until Make(I)[0] = 3;
  repeat
    I := I + 1;
  until I > 5;
  WriteLn(G, ' ', I);
end.
