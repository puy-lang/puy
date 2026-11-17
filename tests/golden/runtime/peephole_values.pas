program PeepholeValues;

uses System.SysUtils;

// What variables hold where codegen's rules change the instructions (design/vm.md 3.15): a
// variable keeps its value when an assignment to it raises, also when the variable was assigned
// just before, and a global read right after it is stored gives the stored value while the
// register it was stored from is reused by the next temporary

var
  G: Integer;

function Step: Boolean;
begin
  Inc(G);
  Result := G <= 2;
end;

procedure Run;
var
  X, T, Y, Z, K: Integer;
begin
  X := MaxInt;
  try
    X := X + 1;
  except
    on E: EIntOverflow do
      WriteLn('X = ', X);
  end;
  X := 7;
  Y := 0;
  T := 0;
  try
    T := X;
    T := X div Y;
  except
    on E: EDivByZero do
      WriteLn('T = ', T);
  end;
  Z := 5;
  try
    Z := 100 div Y + Z;
  except
    on E: EDivByZero do
      WriteLn('Z = ', Z);
  end;
  K := 3;
  K := 5 * K;
  K := K - 1;
  K := K mod 4 + K div 4;
  WriteLn('K = ', K);
end;

begin
  Run;
  G := 0;
  while Step do
    Write(G, ' ');
  WriteLn(G);
end.
