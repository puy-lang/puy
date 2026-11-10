program ArrayInsertDeleteRange;

// Insert / Delete of a dynamic array (types.md 8.6): a position outside the array is
// `ERangeError` whatever `{$R}` says (Insert takes 0..Length, Delete 0..Length - 1), a count
// past the end is cut, and `Count <= 0` does nothing without looking at the index. The last
// error is uncaught.

{$R-}

procedure Show(const A: array of Integer);
var
  I: Integer;
begin
  Write(Length(A), ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn;
end;

var
  A: TArray<Integer>;
begin
  A := [1, 2, 3];
  try
    Insert(9, A, 4);
  except
    on E: ERangeError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    Insert([8, 9], A, -1);
  except
    on E: ERangeError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    Delete(A, 3, 1);
  except
    on E: ERangeError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    Delete(A, -1, 1);
  except
    on E: ERangeError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  Show(A);
  Delete(A, 5, 0);
  Delete(A, -5, -2);
  Delete(A, 1, 100);
  Show(A);
  A := nil;
  Delete(A, 0, 1);
end.
