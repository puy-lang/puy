program LendTouch;

// Touching an element or field lent to a `var` parameter through another path during the call
// is an `EAliasError` (types.md chapter 3): reading or writing it through a global or another
// parameter, and copying or moving the array that holds it.

type
  TItem = record
    X, Y: Integer;
  end;

const
  Whats: array[0..7] of string = ('read', 'write', 'copy', 'slice', 'concat', 'insert',
    'delete', 'other');

var
  A: array of Integer;
  Items: array of TItem;
  B: array of Integer;

procedure Try1(const What: string; var X: Integer);
begin
  try
    case What of
      'read': WriteLn(A[1]);
      'write': A[1] := 9;
      'copy': B := Copy(A);
      'slice': B := A[0..1];
      'concat': B := A + [4];
      'insert': Insert(4, A, 0);
      'delete': Delete(A, 2, 1);
      'other': A[0] := A[2] + 10;
    end;
    WriteLn(What, ': ok');
  except
    on E: EAliasError do
      WriteLn(What, ': ', E.Message);
  end;
  X := X + 1;
end;

procedure ReadArg(const Arr: array of Integer; var X: Integer);
begin
  X := 5;
  try
    WriteLn(Arr[0]);
  except
    on E: EAliasError do
      WriteLn('argument: ', E.Message);
  end;
end;

procedure ReadField(var X: Integer);
begin
  WriteLn(Items[0].Y);
  try
    WriteLn(Items[0].X);
  except
    on E: EAliasError do
      WriteLn('field: ', E.Message);
  end;
  X := 3;
end;

begin
  A := [1, 2, 3];
  for var W in Whats do
    Try1(W, A[1]);
  WriteLn(A[0], ' ', A[1], ' ', A[2]);
  ReadArg(A, A[0]);
  WriteLn(A[0]);
  SetLength(Items, 1);
  Items[0].Y := 4;
  ReadField(Items[0].X);
  WriteLn(Items[0].X);
end.
