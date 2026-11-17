program GenericConstraintOps;

uses
  System.SysUtils;

// The operations of a generic body that its type parameters' constraints must allow
// (generics.md 3.3, 3.5): E0762 with the note of each kind for `Free` / `Destroy` /
// `FreeAndNil` when `T` may be a reference counted class (`class` alone, `TObject`), `=`, `nil`
// and `Assigned` without a `class` or interface constraint, a member no constraint declares, a
// member two interface constraints declare, `T.Create` without `constructor`, `is` with a class
// when only an interface constraint is given, and a conversion to a class the constraints do not
// guarantee. A type parameter passed to another generic declaration must guarantee its
// constraints: E0759 at the type argument, with the constraint to add.

{$WARN unused-variable OFF}

type
  IA = interface
    procedure Run;
    procedure OnlyA;
  end;

  IB = interface
    procedure Run;
  end;

  IC = interface(IA)
  end;

  TBase = class
    procedure Show;
  end;

  TOther = class
  end;

  TList<U: class> = class
  end;

  TOwned<U: class, NotRefCounted> = class
  end;

  TMake<U: class, constructor> = class
  end;

  TPick<U: TBase> = class
  end;

  TUseA<U: IA> = class
  end;

  TValues<U: record> = class
  end;

  TCheck<T: class; R: record; I: IA, IB; N; C: IC; O: TObject; B: TBase> = class
    FItem: T;
    FRec: R;
    FIntf: I;
    FAny: N;
    FC: C;
    FObj: O;
    FBase: B;
    procedure Run;
    procedure Pass;
  end;

procedure TBase.Show;
begin
end;

procedure TCheck<T, R, I, N, C, O, B>.Run;
var
  X: TOther;
begin
  FItem.Free;
  FObj.Destroy;
  FreeAndNil(FItem);
  if FRec = FRec then
    Exit;
  if FAny <> nil then
    Exit;
  if Assigned(FAny) then
    Exit;
  FItem.Show;
  FBase.Show; // a member of the class type constraint
  FIntf.Run;
  FIntf.OnlyA; // declared in `IA` only
  FC.Run; // `IC` inherits `Run` from `IA`
  FItem := T.Create;
  if FIntf is TBase then
    Exit;
  X := FBase;
end;

procedure TCheck<T, R, I, N, C, O, B>.Pass;
var
  LT: TList<T>;
  LR: TList<R>;
  LB: TList<B>;
  OT: TOwned<T>;
  OB: TOwned<B>;
  OO: TOwned<O>;
  MT: TMake<T>;
  PT: TPick<T>;
  PB: TPick<B>;
  UI: TUseA<I>;
  UC: TUseA<C>;
  UN: TUseA<N>;
  VR: TValues<R>;
  VB: TValues<B>;
begin
end;

procedure Free<T: class>(var X: T);
begin
  X.Free;
end;

procedure Churn<T: class, constructor>;
begin
  T.Create.Free; // a member right after `T.Create`
end;

begin
end.
