program GenericMemberHeadingNesting;

// The member headings of an instantiation are made with it (generics.md 4.2), so a method whose
// result type nests the type in itself is E0764 at its declaration, even when nothing uses the
// type or calls the method (4.4).

type
  TFoo<T> = class
    function Deep: TFoo<TFoo<T>>;
  end;

function TFoo<T>.Deep: TFoo<TFoo<T>>;
begin
  Result := nil;
end;

begin
end.
