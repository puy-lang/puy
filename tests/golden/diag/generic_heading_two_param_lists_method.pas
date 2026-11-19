program GenericHeadingTwoParamListsMethod;

// A second type parameter list after the method's name in a method's body heading
// (`TA.M<T><U>`) is a syntax error (E0400), as after a routine's name (grammar.md 1.3).

type
  TA = class
    procedure M<T>;
  end;

procedure TA.M<T><U>;
begin
end;

begin
end.
