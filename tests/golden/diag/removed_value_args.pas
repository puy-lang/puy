program RemovedValueArgs;

// A routine or procedural value written as the argument of a call whose callee is in error (a
// removed function `Addr(F)`, `Dispose(P)`, or an undeclared name) gets only the callee's error,
// not E0704 / E0725 for calling it (objects.md 9.10, 9.11, 9.14); `TObject`'s built-in destructor
// is not a value either (E0752, 9.2).

type
  TEvent = procedure of object;

function Id(X: Integer): Integer;
begin
  Result := X;
end;

procedure Run;
begin
end;

var
  R: TProc;
  O: TObject;
  E: TEvent;
begin
  R := Run;
  R := Addr(Id);    // E3004 only
  R := Addr(R);     // E3004 only
  Dispose(Run);     // E3004 only
  Undecl(Id);       // E0700 only
  R := Undecl2(R);  // E0700 only
  O := TObject.Create;
  E := O.Destroy;   // E0752
  O.Free;
end.
