// The first parts of System.Generics.Collections (docs/spec/collections.md chapter 4) and the
// exceptions of System.SysUtils written in Pascal (3.2): TPair<K, V>, a user class overriding
// TEnumerable<T>.ToArray iterated with for..in (a copy is walked, so changing the source during
// the loop does not change what is enumerated), and EKeyNotFound caught as EListError.
program StdlibCollectionsBase;

uses System.SysUtils, System.Generics.Collections;

type
  TDigits = class(TEnumerable<Integer>)
  public
    Items: TArray<Integer>;
    function ToArray: TArray<Integer>; override;
  end;

function TDigits.ToArray: TArray<Integer>;
begin
  Result := Copy(Items);
end;

var
  P: TPair<string, Integer>;
  Q: TPair<Integer, TPair<string, Integer>>;
  D: TDigits;
  E: TEnumerable<Integer>;
  X: Integer;
begin
  P := TPair<string, Integer>.Create('one', 1);
  WriteLn(P.Key, ' ', P.Value);
  Q := TPair<Integer, TPair<string, Integer>>.Create(2, P);
  WriteLn(Q.Key, ' ', Q.Value.Key);
  WriteLn(KeyEquals(P, Q.Value));

  D := TDigits.Create;
  D.Items := [1, 2, 3];
  E := D;
  for X in E do
  begin
    D.Items := [7];
    Write(X, ' ');
  end;
  WriteLn;
  for X in E do
    Write(X, ' ');
  WriteLn;
  E.Free;

  try
    raise EKeyNotFound.Create('no such key');
  except
    on Ex: EListError do
      WriteLn(Ex.ClassName, ': ', Ex.Message);
  end;
  try
    raise EListError.Create('list');
  except
    on Ex: EKeyNotFound do
      WriteLn('not here');
    on Ex: Exception do
      WriteLn(Ex.ClassName, ': ', Ex.Message);
  end;
end.
