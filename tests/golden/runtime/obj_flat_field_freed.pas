program ObjFlatFieldFreed;

// A record kept flat in a freed instance's field is not reached through a stale handle, also
// when another block took the instance's place in the heap: `EAccessViolation`
// (objects.md 14.1, 14.3; bytecode.md 1.5: a cursor's container may be a handle).

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;
  THolder = class
    P: TPoint;
  end;

var
  H, G: THolder;
  Q: TPoint;
  A: array of Integer;

begin
  H := THolder.Create;
  G := H;
  H.Free;
  SetLength(A, 2);
  A[0] := 5;
  Q.X := 1;
  try
    G.P := Q;
  except
    on E: EAccessViolation do
      WriteLn('write: ', E.Message);
  end;
  try
    Q := G.P;
  except
    on E: EAccessViolation do
      WriteLn('read: ', E.Message);
  end;
  WriteLn(A[0], ' ', A[1]);
end.
