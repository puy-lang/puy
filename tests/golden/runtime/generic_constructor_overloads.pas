program GenericConstructorOverloads;

// `T.Create` of the `constructor` constraint (generics.md 3.1, 4.3) calls the constructor that
// `TFoo.Create` without arguments calls, chosen among the overloads found as the name is found:
// a class whose `Create` overloads take arguments reaches an ancestor's `Create` without
// parameters, or one whose parameters all have default values. Each line made through
// `TMake<T>` matches the line of the direct call before it.

type
  TBase = class
    constructor Create;
  end;

  TOverloads = class(TBase)
    constructor Create(X: Integer); overload;
  end;

  TDefaultBase = class
    constructor Create(X: Integer = 5);
  end;

  TStringOverload = class(TDefaultBase)
    constructor Create(const S: string); overload;
  end;

  TDeeper = class(TStringOverload)
    constructor Create(A, B: Integer); overload;
  end;

  TMake<T: class, NotRefCounted, constructor> = class
    procedure Run;
  end;

constructor TBase.Create;
begin
  WriteLn('TBase.Create of ', ClassName);
end;

constructor TOverloads.Create(X: Integer);
begin
  WriteLn('TOverloads.Create(', X, ')');
end;

constructor TDefaultBase.Create(X: Integer);
begin
  WriteLn('TDefaultBase.Create(', X, ') of ', ClassName);
end;

constructor TStringOverload.Create(const S: string);
begin
  WriteLn('TStringOverload.Create(', S, ')');
end;

constructor TDeeper.Create(A, B: Integer);
begin
  WriteLn('TDeeper.Create(', A, ', ', B, ')');
end;

procedure TMake<T>.Run;
begin
  T.Create.Free;
end;

procedure Check<T: class, NotRefCounted, constructor>;
var
  M: TMake<T>;
begin
  M := TMake<T>.Create;
  M.Run;
  M.Free;
end;

begin
  TOverloads.Create.Free;
  Check<TOverloads>;
  TStringOverload.Create.Free;
  Check<TStringOverload>;
  TDeeper.Create.Free;
  Check<TDeeper>;
end.
