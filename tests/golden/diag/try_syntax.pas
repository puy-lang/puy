// Syntax errors of try statements (docs/spec/exceptions.md chapter 4): a `try` without `except`
// or `finally`, `try..except..finally`, a statement mixed into the handlers, a missing `do` and a
// missing `;` between handlers.
program TrySyntax;

begin
  try
    WriteLn(1);
  end;
  try
    WriteLn(2);
  except
    WriteLn(3);
  finally
    WriteLn(4);
  end;
  try
    WriteLn(5);
  except
    on E: Exception do
      WriteLn(6);
    WriteLn(7);
  end;
  try
    WriteLn(8);
  except
    on E: Exception WriteLn(9);
  end;
  try
    WriteLn(10);
  except
    on E: EConvertError do WriteLn(11)
    on E: Exception do WriteLn(12);
  end;
end.
