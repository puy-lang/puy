program ForParam;

// A value parameter can be a loop variable (semantics.md 4.5; same as Delphi / FPC)

procedure Count(I: Integer; N: Integer);
begin
  for I := 1 to N do
    Write(I, ' ');
  WriteLn;
end;

function Sum(C: Char; Last: Char): Integer;
begin
  Result := 0;
  for C := C to Last do
    Inc(Result, Ord(C));
end;

begin
  Count(99, 3);
  WriteLn(Sum('a', 'c'));
end.
