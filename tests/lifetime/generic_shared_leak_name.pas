program GenericSharedLeakName;

// A shared function makes its dynamic arrays with the type of the instantiation made first
// (design generics.md 3.3 B; generics.md 7): `TList<TFoo>` and `TList<TBar>` share `Add`, so the
// array of a leaked `TList<TBar>` is reported as `TArray<GenericSharedLeakName.TFoo>`. The
// instance itself is made by `New` with its own class (an identity operand), so it is reported
// by its own name.

type
  TList<T> = class
    FItems: TArray<T>;
    procedure Add(const X: T);
  end;

  TFoo = class
  end;
  TBar = class
  end;

procedure TList<T>.Add(const X: T);
begin
  FItems := FItems + [X];
end;

var
  Foos: TList<TFoo>;
  Bars: TList<TBar>;
begin
  Foos := TList<TFoo>.Create;
  Foos.Add(nil);
  Foos.Free;
  Bars := TList<TBar>.Create;
  Bars.Add(nil);
  WriteLn(Length(Bars.FItems));
end.
