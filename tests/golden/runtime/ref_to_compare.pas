program RefToCompare;

// `=` / `<>` of `reference to` values (objects.md 9.5): two evaluations of the same anonymous
// method are different values (identity of the block), a copy of one value is equal to it, and a
// value holding a routine or a method reference compares like the routine or the reference.

type
  TRun = reference to procedure;
  TPlain = procedure;
  TEvent = procedure of object;

  TThing = class
    procedure Ping;
    procedure Pong;
  end;

procedure TThing.Ping;
begin
end;

procedure TThing.Pong;
begin
end;

procedure Hello;
begin
end;

procedure Bye;
begin
end;

function Mk: TRun;
begin
  Result := procedure begin WriteLn('run') end;
end;

var
  A, B: TRun;
  P: TPlain;
  E: TEvent;
  T1, T2: TThing;
begin
  WriteLn(Mk() = Mk());
  A := Mk();
  B := A;
  WriteLn(A = B, ' ', A <> B);
  A := Hello;
  B := Hello;
  P := Hello;
  WriteLn(A = B, ' ', A = Bye);
  B := P;
  WriteLn(A = B);
  T1 := TThing.Create;
  T2 := TThing.Create;
  A := T1.Ping;
  B := T1.Ping;
  WriteLn(A = B, ' ', A = T1.Pong, ' ', A = T2.Ping);
  E := T1.Ping;
  B := E;
  WriteLn(A = B);
  A := nil;
  WriteLn(A = nil, ' ', B <> nil);
  T1.Free;
  T2.Free;
end.
