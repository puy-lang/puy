program KeyEqualsErrors;

// Errors of KeyEquals / KeyHash (equality.md 4): the type argument cannot be inferred from
// arguments of different weak types or from `nil` (E0763); they are not routines, so they cannot
// be procedural values (E0752); they take one type argument (E0761) and a fixed number of
// arguments (E0705); the arguments convert to the type argument (E0702); a call written
// without parentheses before `+` needs them (E0761, as for generic routines); and they are not
// constant expressions, since a hash changes between processes.

type
  TEq = function(const A, B: Integer): Boolean;
  TEqRef = reference to function(const A, B: Integer): Boolean;

const
  H = KeyHash(1);

var
  F: TEq;
  R: TEqRef;
  B: Boolean;
  S: string;
begin
  B := KeyEquals(1, 2.5);
  B := KeyEquals(nil, nil);
  F := KeyEquals;
  R := KeyEquals<Integer>;
  B := KeyEquals<Integer, Integer>(1, 2);
  B := KeyEquals(1);
  B := KeyEquals;
  WriteLn(KeyHash(1, 2));
  B := KeyEquals<Integer>(1, S);
  WriteLn(KeyHash<Integer> + 1);
end.
