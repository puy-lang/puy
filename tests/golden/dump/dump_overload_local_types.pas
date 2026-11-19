program DumpOverloadLocalTypes;

// The local types of overloaded routines are shown under each one's heading (semantics.md
// chapter 9): `TA` under `P`, `TB` under `P#2`.

procedure P(N: Integer); overload;
type
  TA = record
    X: Integer;
  end;
var
  L: TA = Default(TA);
begin
  L.X := N;
  WriteLn(L.X);
end;

procedure P(const S: string); overload;
type
  TB = record
    Y: string;
  end;
var
  L: TB = Default(TB);
begin
  L.Y := S;
  WriteLn(L.Y);
end;

begin
  P(1);
  P('a');
end.
