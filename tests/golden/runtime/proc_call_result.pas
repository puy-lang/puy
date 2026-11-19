program ProcCallResult;

// A call of a function whose result is a procedural value can be called again (objects.md 9.4):
// `F()()` calls `F` and then its result, and `G()()()` goes one step further. `O.H()` as a
// statement reads the property and calls its value; `O.H` without parentheses passes the value.

type
  TRun = reference to procedure;
  TMake = reference to function: TRun;
  TObj = class
    function GetH: TRun;
    property H: TRun read GetH;
  end;

procedure Hello;
begin
  WriteLn('hello');
end;

function TObj.GetH: TRun;
begin
  WriteLn('GetH');
  Result := Hello;
end;

function F: TRun;
begin
  WriteLn('F');
  Result := Hello;
end;

function G: TMake;
begin
  WriteLn('G');
  Result := F;
end;

procedure Run(const R: TRun);
begin
  WriteLn('Run');
  R();
end;

var
  O: TObj;
  R: TRun;
begin
  O := TObj.Create;
  WriteLn('-- O.H()');
  O.H();
  WriteLn('-- O.H');
  O.H;
  WriteLn('-- Run(O.H)');
  Run(O.H);
  WriteLn('-- R := O.H');
  R := O.H;
  R();
  WriteLn('-- F()()');
  F()();
  WriteLn('-- Run(F())');
  Run(F());
  WriteLn('-- G()()()');
  G()()();
  O.Free;
end.
