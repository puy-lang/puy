program GenericNames;

// The names of type parameters (generics.md 2.2, 3.6): a type parameter declared twice and one
// with the name of a member of its type are E0701; a type parameter hides a type of the same
// name outside the generic type (so `T` below is not `string`); a type parameter used as a value
// is E0708, and so is `NotRefCounted` outside the position of a constraint.

type
  T = string;
  TTwice<T, T> = class
  end;
  TFieldNamed<T> = class
    T: Integer;
  end;
  TMethodNamed<T> = record
    procedure T;
  end;
  THides<T> = class
    FItem: T;
    procedure Run;
  end;

procedure THides<T>.Run;
var
  S: NotRefCounted;
begin
  FItem := 'text';
  WriteLn(T);
  WriteLn(NotRefCounted);
end;

var
  S: T;
begin
  S := 'outside';
  WriteLn(S);
end.
