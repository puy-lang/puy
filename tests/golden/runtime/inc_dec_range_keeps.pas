program IncDecRangeKeeps;

// Under `{$R+}` an `Inc` / `Dec` whose result is out of the variable's range raises ERangeError
// and leaves the variable with its value before the call, as an assignment does
// (semantics.md 3.1, 5.1): local variables of `Byte`, a subrange, an enumeration and a distinct
// type, a `Char`, a `var` parameter and a global, read in the handler and in `finally`. An
// `Integer` that overflows under `{$Q+}` keeps its value too, and one that does not is written.

uses System.SysUtils;

type
  TDigit = 0..9;
  TColor = (Red, Green, Blue);
  TSmall = type Byte;

var
  G: Byte;

procedure Locals;
var
  B: Byte;
  D: TDigit;
  C: TColor;
  S: TSmall;
  Ch: Char;
  N: Integer;
begin
  B := 255;
  try
    Inc(B);
  except
    on E: ERangeError do
      WriteLn('Byte Inc: ', B, ' (', E.Message, ')');
  end;
  B := 0;
  try
    Dec(B);
  except
    on ERangeError do
      WriteLn('Byte Dec: ', B);
  end;
  B := 250;
  N := 10;
  try
    Inc(B, N);
  except
    on ERangeError do
      WriteLn('Byte Inc by N: ', B);
  end;
  B := 5;
  try
    Dec(B, 300);
  except
    on ERangeError do
      WriteLn('Byte Dec by 300: ', B);
  end;
  D := 9;
  try
    Inc(D);
  except
    on ERangeError do
      WriteLn('subrange Inc: ', D);
  end;
  D := 0;
  try
    try
      Dec(D);
    finally
      WriteLn('subrange Dec in finally: ', D);
    end;
  except
    on ERangeError do
      WriteLn('subrange Dec caught: ', D);
  end;
  C := TColor.Blue;
  try
    Inc(C);
  except
    on ERangeError do
      WriteLn('enum Inc: ', Ord(C));
  end;
  S := 255;
  try
    Inc(S);
  except
    on ERangeError do
      WriteLn('distinct Inc: ', S);
  end;
  S := 0;
  try
    Dec(S, 2);
  except
    on ERangeError do
      WriteLn('distinct Dec: ', S);
  end;
  Ch := #65535;
  try
    Inc(Ch);
  except
    on ERangeError do
      WriteLn('Char Inc: ', Ord(Ch));
  end;
  N := MaxInt;
  try
    Inc(N);
  except
    on EIntOverflow do
      WriteLn('Integer Inc: ', N);
  end;
  B := 1;
  D := 5;
  try
    Inc(B);
    Inc(B, 3);
    Dec(D);
    WriteLn('in range: ', B, ' ', D);
  except
    on ERangeError do
      WriteLn('unexpected');
  end;
end;

procedure ByRef(var X: Byte);
begin
  Inc(X);
end;

begin
  Locals;
  G := 255;
  try
    ByRef(G);
  except
    on ERangeError do
      WriteLn('var parameter Inc: ', G);
  end;
  G := 0;
  try
    Dec(G);
  except
    on ERangeError do
      WriteLn('global Dec: ', G);
  end;
end.
