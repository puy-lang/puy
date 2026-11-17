program RefcountCycle;

// Two instances of reference counted classes that refer to each other are never destroyed: the
// cycle is reported as a leak by the classes' names (objects.md 3.3).

type
  TPeer = class(TRefObject)
  public
    Other: TPeer;
  end;

procedure Run;
var
  A, B: TPeer;
begin
  A := TPeer.Create;
  B := TPeer.Create;
  A.Other := B;
  B.Other := A;
  WriteLn(Assigned(A.Other), Assigned(B.Other));
end;

begin
  Run;
  WriteLn('end');
end.
