// puy-test: test-ext
// `PasError::argument` of an extension is raised as the built-in EArgumentException
// (`docs/spec/extensions.md` 5.3), caught by `on E: EArgumentException` without
// `uses System.SysUtils`: an invalid name given to `CallCtx::set_env_var`, and a callback called
// with more arguments than its signature takes.
program ExtArgumentException;

uses TestExt;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

begin
  try
    SetEnv('A=B', 'x');
  except
    on E: EArgumentException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    SetEnv('', 'x');
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    WriteLn(ApplyTwo(Twice));
  except
    on E: EArgumentException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  SetEnv('it''s=', 'x');
end.
