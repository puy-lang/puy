// A type parameter with a class constraint in `X is T`, `X as T`, `on X: T do` and `T.ClassName`
// (docs/spec/generics.md 3.3): each instantiation checks against its type argument, a descendant
// of the argument passes, and the routines of two instantiations are not shared (the failed
// `as` at the end is reported in `Expect<GenericClassParamChecks.TDog>`).
program GenericClassParamChecks;

uses
  System.SysUtils;

type
  TAnimal = class
  end;

  TDog = class(TAnimal)
  end;

  TPuppy = class(TDog)
  end;

  TCat = class(TAnimal)
  end;

  EParse = class(Exception)
  end;

  EParseNumber = class(EParse)
  end;

  TFilter<T: TAnimal> = class
    function Count(const Items: array of TAnimal): Integer;
    function Name: string;
  end;

function TFilter<T>.Count(const Items: array of TAnimal): Integer;
var
  A: TAnimal;
begin
  Result := 0;
  for A in Items do
    if A is T then
      Inc(Result);
end;

function TFilter<T>.Name: string;
begin
  Result := T.ClassName;
end;

function Expect<T: class>(O: TObject): T;
begin
  Result := O as T;
end;

function Catch<E: Exception>(const P: TProc): string;
begin
  try
    P();
    Result := 'nothing';
  except
    on X: E do
      Result := E.ClassName + ' caught ' + X.ClassName + ': ' + X.Message;
    on X: Exception do
      Result := E.ClassName + ' missed ' + X.ClassName;
  end;
end;

function KindOf<T: class>(O: TObject): string;
var
  F: TFunc<Boolean>;
begin
  // `is` inside an anonymous method of the generic routine
  F :=
    function: Boolean
    begin
      Result := O is T;
    end;
  Result := BoolToStr(F(), True);
end;

var
  Dogs: TFilter<TDog>;
  Cats: TFilter<TCat>;
  Zoo: array of TAnimal;
  A: TAnimal;
begin
  Zoo := [TDog.Create, TPuppy.Create, TCat.Create, TAnimal.Create];
  Dogs := TFilter<TDog>.Create;
  Cats := TFilter<TCat>.Create;
  WriteLn(Dogs.Name, ' ', Dogs.Count(Zoo));
  WriteLn(Cats.Name, ' ', Cats.Count(Zoo));
  WriteLn(Expect<TDog>(Zoo[1]).ClassName);
  WriteLn(Expect<TObject>(Zoo[2]).ClassName);
  WriteLn(Expect<TDog>(nil) = nil);
  WriteLn(KindOf<TDog>(Zoo[1]), ' ', KindOf<TCat>(Zoo[1]));
  WriteLn(Catch<EParse>(procedure begin raise EParseNumber.Create('bad digit') end));
  WriteLn(Catch<EParseNumber>(procedure begin raise EParse.Create('bad') end));
  WriteLn(Catch<EConvertError>(procedure begin end));
  Dogs.Free;
  Cats.Free;
  try
    WriteLn(Expect<TDog>(Zoo[2]).ClassName);
  finally
    for A in Zoo do
      A.Free;
  end;
end.
