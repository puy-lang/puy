program SubrangeRange;

// Range checks of subrange and narrow types under {$R+} (types.md 8.2, semantics.md 2.4) that
// FPC gives too: an out of range assignment, `Inc`, a parameter and an array index raise
// ERangeError, caught by `on ERangeError` (the message is in golden/), and the same assignment
// under {$R-} keeps the low bits of narrow integer types.

uses System.SysUtils;

type
  TDigit = 0..9;
  TPercent = 0..100;

var
  D: TDigit;
  P: TPercent;
  B: Byte;
  I: Integer;
  A: array[TDigit] of Integer;

procedure Take(X: TDigit);
begin
  WriteLn('took ', X);
end;

procedure Try1(N: Integer);
begin
  try
    D := N;
    WriteLn('assigned ', D);
  except
    on ERangeError do
      WriteLn('range error for ', N);
  end;
end;

begin
  Try1(0);
  Try1(9);
  Try1(10);
  Try1(-1);
  D := 9;
  try
    Inc(D);
    WriteLn('no error');
  except
    on ERangeError do
      WriteLn('Inc range error, D = ', D);
  end;
  I := 12;
  try
    Take(I);
  except
    on ERangeError do
      WriteLn('parameter range error');
  end;
  try
    A[I] := 1;
  except
    on ERangeError do
      WriteLn('index range error');
  end;
  I := 50;
  P := I * 2;
  WriteLn(P);
  try
    P := I * 3;
  except
    on ERangeError do
      WriteLn('percent range error, P = ', P);
  end;
  I := 256 + 7;
  try
    B := I;
  except
    on E: Exception do
      WriteLn('byte: ', E.ClassName);
  end;
{$R-}
  B := I;
  WriteLn(B);
{$R+}
end.
