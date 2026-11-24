program GenericClassParamCheckErrors;

// The checks with a type parameter (generics.md 3.3): without a class constraint, `X is T` is
// E0702, `on X: T do` is E0733 with the constraint to add (also when the constraint is a class
// that is not an exception) and `T.ClassName` is E0762; a class that can never be the
// constraint's class or a descendant is E0702; a handler of a type parameter after a handler of
// an ancestor of its constraint is W0019, and a handler after one of a type parameter is not.

uses
  System.SysUtils;

type
  TAnimal = class
  end;

  TStone = class
  end;

procedure Unconstrained<T>(O: TObject);
begin
  if O is T then
    WriteLn(T.ClassName);
  try
  except
    on X: T do
      begin end;
  end;
end;

procedure NotException<T: class>;
begin
  try
  except
    on X: T do
      begin end;
  end;
end;

procedure Never<T: TAnimal>(S: TStone);
begin
  if S is T then
    begin end;
end;

procedure Order<E: EConvertError>;
begin
  try
  except
    on X: E do
      begin end;
    on X: EConvertError do
      begin end;
  end;
  try
  except
    on X: Exception do
      begin end;
    on X: E do
      begin end;
  end;
end;

begin
end.
