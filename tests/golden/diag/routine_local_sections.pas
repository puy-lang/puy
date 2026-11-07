program RoutineLocalSections;

// type / label right after the header are local sections and start the body (grammar.md 1.3).
// They are not bodyless declarations (E0411); type gives one E3200 and label one E3001

procedure P;
type
  TCount = Integer;
var
  I: Integer;
begin
  I := 1;
  WriteLn(I);
end;

procedure Q;
label
  Done;
begin
  WriteLn('q');
end;

begin
  P;
  Q;
end.
