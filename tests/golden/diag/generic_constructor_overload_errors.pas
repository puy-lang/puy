program GenericConstructorOverloadErrors;

// The `constructor` constraint (generics.md 3.1) is satisfied as `TFoo.Create` without arguments
// is resolved: E0759 when a `Create` without `overload` hides an ancestor's `Create` without
// parameters, when two overloads can be called without arguments (the direct call is ambiguous,
// E0734), and when the only one without parameters is not public. A body that calls `T.Create`
// is still instantiated for such a type argument, without an internal error.

type
  TBase = class
    constructor Create;
  end;

  THides = class(TBase)
    constructor Create(X: Integer);
  end;

  TAmbiguous = class(TBase)
    constructor Create(X: Integer = 0); overload;
  end;

  TProtected = class
  protected
    constructor Create; overload;
  public
    constructor Create(X: Integer); overload;
  end;

  TMake<T: class, constructor> = class
    function Make: T;
  end;

constructor TBase.Create;
begin
end;

constructor THides.Create(X: Integer);
begin
end;

constructor TAmbiguous.Create(X: Integer);
begin
end;

constructor TProtected.Create;
begin
end;

constructor TProtected.Create(X: Integer);
begin
end;

function TMake<T>.Make: T;
begin
  Result := T.Create;
end;

var
  H: TMake<THides>;
  A: TMake<TAmbiguous>;
  P: TMake<TProtected>;
begin
  H.Make.Free;
  A.Make.Free;
  P.Make.Free;
end.
