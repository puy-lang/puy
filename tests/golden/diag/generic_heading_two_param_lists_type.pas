program GenericHeadingTwoParamListsType;

// A second type parameter list after the type's name in a method's body heading
// (`TA<T><U>.M`) is a syntax error (E0400), as after a routine's name (grammar.md 1.3).

type
  TA<T> = class
    procedure M;
  end;

procedure TA<T><U>.M;
begin
end;

begin
end.
