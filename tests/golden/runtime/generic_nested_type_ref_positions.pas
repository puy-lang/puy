program GenericNestedTypeRefPositions;

// A type nested in an instantiation written after the type arguments (generics.md 2.5) in the
// other type positions: a class's parent and interfaces, an interface's parent, a constraint,
// `class of`, a distinct type's original, a helper's type and parent, and `on E:`.

uses System.SysUtils;

type
  TOuter<T> = class
  public
    type
      TBase = class
      public
        Value: T;
      end;
      IRun = interface
        procedure Run;
      end;
      ERun = class(Exception);
      TAmount = Integer;
  end;

  TDerived = class(TOuter<Integer>.TBase, TOuter<Integer>.IRun)
  public
    procedure Run;
  end;

  IRunMore = interface(TOuter<Integer>.IRun)
    procedure More;
  end;

  TMore = class(TRefObject, IRunMore)
  public
    procedure Run;
    procedure More;
  end;

  TBaseClass = class of TOuter<Integer>.TBase;
  TCount = type TOuter<Integer>.TAmount;

  TBaseHelper = class helper for TOuter<Integer>.TBase
    function Twice: Integer;
  end;

  TBox<X: TOuter<Integer>.TBase> = class
  public
    function Read(Item: X): Integer;
  end;

procedure TDerived.Run;
begin
  WriteLn('run ', Value);
end;

procedure TMore.Run;
begin
  WriteLn('more: run');
end;

procedure TMore.More;
begin
  WriteLn('more: more');
end;

function TBaseHelper.Twice: Integer;
begin
  Result := Value * 2;
end;

function TBox<X>.Read(Item: X): Integer;
begin
  Result := Item.Value;
end;

var
  D: TDerived;
  R: TOuter<Integer>.IRun;
  M: IRunMore;
  C: TBaseClass;
  N: TCount;
  B: TBox<TDerived>;
begin
  D := TDerived.Create;
  D.Value := 21;
  R := D;
  R.Run;
  WriteLn(D.Twice);

  M := TMore.Create;
  M.Run;
  M.More;
  R := M;
  R.Run;

  C := TDerived;
  WriteLn(C.ClassName);
  N := 5;
  WriteLn(N + 1);

  B := TBox<TDerived>.Create;
  WriteLn(B.Read(D));
  B.Free;

  try
    raise TOuter<Integer>.ERun.Create('boom');
  except
    on E: TOuter<Integer>.ERun do
      WriteLn('caught ', E.Message);
  end;
  D.Free;
end.
