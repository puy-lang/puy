program ForParamModes;

// Only value parameters can be loop variables. A const parameter is E0707, a var / out parameter is E0722,
// and reading it after the loop is E2004 (semantics.md 4.5, 4.4)

procedure P(const C: Integer; var V: Integer; out O: Integer; I: Integer);
begin
  O := 0;
  for C := 1 to 3 do begin end;
  for V := 1 to 3 do begin end;
  for O := 1 to 3 do begin end;
  for I := 1 to 3 do begin end;
  WriteLn(I);
end;

begin
end.
