program RefcountUnusedWarnings;

// W0001 and W0002 do not report a variable of a reference counted class or of exactly
// `TObject` (semantics.md 4.6): holding the instance and letting it go (`X := nil`) decide when
// its destructor runs. A variable of a class managed by hand is reported as before.

type
  TGuard = class(TRefObject)
  end;

  TPlain = class
  end;

procedure Run;
var
  Guard: TGuard;
  Held: TGuard;
  O: TObject;
  P: TPlain;
begin
  Guard := TGuard.Create;
  Held := TGuard.Create;
  Held := nil;
  O := TGuard.Create;
  O := nil;
  P := nil;
end;

begin
  Run;
end.
