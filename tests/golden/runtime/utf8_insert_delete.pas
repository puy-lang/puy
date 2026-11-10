program Utf8InsertDelete;

// Insert / Delete of a `UTF8String` (types.md 8.5) work in bytes and cut positions outside the
// value like a string; a position inside a multibyte character is `EEncodingError` (the Insert
// position, either end of the Delete range). The last error is uncaught.

var
  U: UTF8String;
begin
  U := 'aé日';
  Insert('-', U, 2);
  WriteLn(U, ' ', Length(U));
  Insert('<', U, 0);
  Insert('>', U, 100);
  WriteLn(U, ' ', Length(U));
  Delete(U, 4, 2);
  WriteLn(U, ' ', Length(U));
  Delete(U, 100, 1);
  Delete(U, 2, 0);
  WriteLn(U);
  try
    Insert('x', U, 6);
  except
    on E: EEncodingError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    Delete(U, 4, 1);
  except
    on E: EEncodingError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  Delete(U, 6, 1);
end.
