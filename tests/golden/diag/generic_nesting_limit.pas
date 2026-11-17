program GenericNestingLimit;

// An instantiation whose body asks for a deeper one of the same type without end: E0764 once,
// where the user wrote the first type arguments, with the path of the instantiations in the
// notes (generics.md 4.4). A nesting within the limit is not reported.

type
  TDeep<T> = class
    procedure Go;
  end;

  TWrap<T> = record
    Value: T;
  end;

procedure TDeep<T>.Go;
var
  D: TDeep<TArray<T>>;
begin
  D := TDeep<TArray<T>>.Create;
  D.Go;
  D.Free;
end;

var
  D: TDeep<Integer>;
  E: TDeep<string>;
  W: TWrap<TWrap<TWrap<TWrap<Integer>>>>;
begin
  D := TDeep<Integer>.Create;
  D.Go;
  E := TDeep<string>.Create;
  E.Go;
end.
