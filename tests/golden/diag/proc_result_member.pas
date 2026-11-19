program ProcResultMember;

// A member of a procedural value that is not a variable (objects.md 9.10): the result of a
// function of a procedural, method or anonymous method type, an element and a property have no
// members (E0700, as for a variable). For `Invoke` the note calls the value: `F()()` for a
// function written with or without parentheses, `F(1)()`, `A[0]()` and `O.H()` otherwise (an
// indexed or default property read by a function too: `H.Items[0]()`, `H[0]()`). A value in
// parentheses or converted to a type has no way to be called in place, and no note.

type
  TAction = procedure;
  TEvent = procedure of object;
  TRun = reference to procedure;

  THolder = class
    function GetRun: TRun;
    function MakeRun: TRun;
    function GetItem(I: Integer): TRun;
    property Run: TRun read GetRun;
    property Items[I: Integer]: TRun read GetItem; default;
  end;

function THolder.GetItem(I: Integer): TRun;
begin
  Result := nil;
end;

function THolder.GetRun: TRun;
begin
  Result := nil;
end;

function THolder.MakeRun: TRun;
begin
  Result := nil;
end;

function MakeAction: TAction;
begin
  Result := nil;
end;

function MakeEvent: TEvent;
begin
  Result := nil;
end;

function MakeRun(N: Integer): TRun;
begin
  Result := nil;
end;

var
  A: TAction;
  Runs: array of TRun;
  H: THolder;
begin
  MakeAction.Foo;
  A.Foo;
  MakeAction.Invoke;
  MakeAction().Invoke;
  MakeEvent.Invoke;
  MakeRun(1).Invoke;
  Runs[0].Invoke;
  H.Run.Invoke;
  H.MakeRun.Invoke;
  H.Items[0].Invoke;
  H[0].Invoke;
  Runs[0].Foo;
  H.Run.Foo;
  (MakeAction).Invoke;
  TAction(MakeAction()).Invoke;
  TRun(MakeRun(1)).Invoke;
end.
