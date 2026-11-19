program ProcCallNoValue;

// A call whose callee is a procedure has no value (objects.md 9.4): calling its result again,
// or using it in an expression, is E0725 at the call that has no value. `O.H()` calls the
// property's value, `F()()` the result of `F`, and `R()` the variable's value; each of them is
// a procedure here, so another `()` or a use as a value has nothing to work on. A call in
// parentheses written as a statement is E0402 alone, without E0725 on top of it.

type
  TRun = reference to procedure;
  TObj = class
    function GetH: TRun;
    procedure P;
    property H: TRun read GetH;
  end;

procedure Hello;
begin
end;

function TObj.GetH: TRun;
begin
  Result := Hello;
end;

procedure TObj.P;
begin
end;

function F: TRun;
begin
  Result := Hello;
end;

procedure Run(const R: TRun);
begin
  R();
end;

var
  O: TObj;
  R: TRun;
  X: Integer;
begin
  O := TObj.Create;
  R := Hello;
  O.H()();          // E0725: `O.H()` calls the value of `H`
  F()()();          // E0725: `F()()` calls the result of `F`
  R()();            // E0725
  X := O.H()();     // E0725
  X := R();         // E0725
  X := Hello();     // E0725
  WriteLn(R());     // E0725
  Run(O.H());       // E0725: `O.H` passes the value
  R := F()();       // E0725
  X := O.P();       // E0725
  (Hello());        // E0402 only: not a statement, and the call is not used as a value
  (R());            // E0402 only
  O.Free;
end.
