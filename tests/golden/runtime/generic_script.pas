// A generic type and a generic routine declared at the top level of a script, and an
// instantiation as the type of a top-level variable (generics.md 2.1).
type
  TBox<T> = record
    V: T;
  end;

function Id<T>(const X: T): T;
begin
  Result := X;
end;

var B: TBox<string> = Default(TBox<string>);
B.V := 'hi';
WriteLn(Id(B.V), ' ', Id<Integer>(3));
