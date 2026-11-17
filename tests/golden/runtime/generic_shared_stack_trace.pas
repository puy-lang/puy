program GenericSharedStackTrace;

// `TBox<TFoo>` and `TBox<TBar>` hold classes managed by hand, so their methods come out the same
// and are shared (generics.md 7); `Check` does not depend on `T`, so `TBox<string>` shares it
// too, while `TBox<string>.Add` counts its strings and keeps its own name. The stack trace of an
// exception raised through the shared function shows the generic name `TBox<T>.Check`, not the
// name of the instantiation made first (`TBox<TFoo>`), although `TBox<TBar>` called it.

uses System.SysUtils;

type
  TBox<T> = class
    FItems: TArray<T>;
    procedure Add(const X: T);
    procedure Check(I: Integer);
  end;

  TFoo = class
  end;
  TBar = class
  end;

procedure TBox<T>.Add(const X: T);
begin
  FItems := FItems + [X];
end;

procedure TBox<T>.Check(I: Integer);
begin
  if (I < 0) or (I >= Length(FItems)) then
    raise ERangeError.Create('no item ' + IntToStr(I));
end;

var
  Foos: TBox<TFoo>;
  Bars: TBox<TBar>;
  Strs: TBox<string>;
begin
  Foos := TBox<TFoo>.Create;
  Foos.Add(nil);
  Foos.Check(0);
  Strs := TBox<string>.Create;
  Strs.Add('a');
  try
    Strs.Check(1);
  except
    on E: ERangeError do
      WriteLn(E.Message);
  end;
  Strs.Free;
  Foos.Free;
  Bars := TBox<TBar>.Create;
  Bars.Add(nil);
  WriteLn('checking');
  try
    Bars.Check(2);
  finally
    Bars.Free;
  end;
end.
