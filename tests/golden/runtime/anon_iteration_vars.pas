program AnonIterationVars;

uses System.SysUtils;

// Variables declared per iteration (objects.md 9.6): an anonymous method capturing the variable of
// `for var I` keeps that iteration's variable (1 2 3); one capturing a loop variable declared
// before the loop and called after the loop reads the value the loop wrote last (Puy's rule);
// an inline variable declared in the loop body is a new variable each iteration, as is the
// variable of `for var X in`; an anonymous method inside an anonymous method that writes a
// variable two routines out writes the routine's own variable.

type
  TProcs = array of TProc;

procedure RunAll(const Procs: TProcs);
var
  P: TProc;
begin
  for P in Procs do
    P();
  WriteLn;
end;

procedure ForVar;
var
  Procs: TProcs;
begin
  SetLength(Procs, 3);
  for var I := 1 to 3 do
    Procs[I - 1] := procedure
      begin
        Write(I, ' ');
      end;
  RunAll(Procs);
end;

procedure DeclaredBefore;
var
  Procs: TProcs;
  I: Integer;
begin
  SetLength(Procs, 3);
  for I := 1 to 3 do
    Procs[I - 1] := procedure
      begin
        Write(I, ' ');
      end;
  RunAll(Procs);
end;

procedure InlineInBody;
var
  Procs: TProcs;
begin
  SetLength(Procs, 3);
  for var I := 0 to 2 do
  begin
    var Name := 'item' + IntToStr(I * 10);
    var Count: Integer;
    Count := I;
    Procs[I] := procedure
      begin
        Write(Name, '/', Count, ' ');
      end;
  end;
  RunAll(Procs);
end;

procedure ForIn;
var
  Procs: TProcs;
  Names: TArray<string>;
  K: Integer;
begin
  SetLength(Procs, 3);
  Names := ['x', 'y', 'z'];
  K := 0;
  for var S in Names do
  begin
    Procs[K] := procedure
      begin
        Write(S, ' ');
      end;
    Inc(K);
  end;
  RunAll(Procs);
end;

procedure Nested;
var
  Total: Integer;
  Outer: TProc;
begin
  Total := 0;
  Outer := procedure
    var
      Inner: TProc;
    begin
      Inner := procedure
        begin
          Total := Total + 5;
        end;
      Inner();
      Inner();
    end;
  Outer();
  WriteLn('Total = ', Total);
end;

begin
  ForVar;
  DeclaredBefore;
  InlineInBody;
  ForIn;
  Nested;
end.
