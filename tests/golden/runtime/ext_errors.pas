// puy-test: test-ext
// The errors of an extension's functions are exceptions raised at the call
// (`docs/spec/extensions.md` 5.3): a class of the extension's unit, a built-in class with its
// property, and the failure of a suspendable function after it waited.
program ExtErrors;

uses TestExt;

begin
  try
    Fail('broken');
  except
    on E: ETestError do
      WriteLn('ETestError: ', E.Message);
  end;
  try
    FailIO(28);
  except
    on E: EInOutError do
      WriteLn(E.ClassName, ': ', E.Message, ' (', E.ErrorCode, ')');
  end;
  try
    FailRange;
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    WriteLn(Add(High(Integer), 1));
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    WriteLn(SlowFail);
  except
    on E: ETestError do
      WriteLn('later: ', E.Message);
  end;
  Fail('uncaught');
end.
