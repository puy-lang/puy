// puy-test: test-ext
// An extension calls Pascal back (`docs/spec/extensions.md` 5.2): values go both ways, an
// exception of the callback is raised again as it is (or handled by the extension), callbacks
// nest, and a callback cannot re-enter an instance the call holds nor wait.
program ExtCallback;

uses System.SysUtils, TestExt;

var
  Total: Integer;

procedure Collect(X: Integer);
begin
  Total := Total + X;
end;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

function Depth(N: Integer): Integer;
begin
  if N = 0 then
    Exit(0);
  Result := Apply(function(X: Integer): Integer
    begin
      Result := Depth(X - 1) + 1;
    end, N);
end;

var
  C: TCounter;
  Base: Integer;
begin
  Base := 5;
  WriteLn(Apply(function(X: Integer): Integer
    begin
      Result := X * X + Base;
    end, 7));
  WriteLn(Apply(Twice, 21));
  WriteLn(ApplyStr(function(const S: string): string
    begin
      Result := S + '!' + S;
    end, 'ab'));
  WriteLn(ApplyStr(function(const S: string): string
    begin
      Result := 'ü' + S;
    end, 'é'));
  try
    Apply(function(X: Integer): Integer
      begin
        raise ETestError.Create('from the callback ' + IntToStr(X));
      end, 3);
  except
    on E: ETestError do
      WriteLn('caught ', E.ClassName, ': ', E.Message);
  end;
  WriteLn(TryApply(function(X: Integer): Integer
    begin
      raise EInvalidOperation.Create('handled by Rust');
    end, 1));
  WriteLn(TryApply(function(X: Integer): Integer
    begin
      Result := X + 1;
    end, 1));
  WriteLn(Depth(10));
  C := TCounter.Create(4);
  Total := 0;
  C.Each(Collect);
  WriteLn('each ', Total);
  try
    C.Each(procedure(X: Integer)
      begin
        C.Increment;
      end);
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn(C.Value);
  try
    Apply(function(X: Integer): Integer
      begin
        Result := SlowAdd(X, 1);
      end, 1);
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
