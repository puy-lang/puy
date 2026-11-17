program InterfaceResolutionAncestor;

// A method resolution clause naming a descendant interface also applies to the entry of the
// ancestor interface declaring the method (objects.md 7.8): `IBar.Run = DoRun` makes `DoRun` the
// `Run` of both `IBar` and `IFoo`, while a clause naming `IFoo` itself wins for `IFoo`.

type
  IFoo = interface
    procedure Run;
    procedure Walk;
  end;

  IBar = interface(IFoo)
    procedure Stop;
  end;

  TX = class(TRefObject, IBar)
  public
    procedure IBar.Run = DoRun;
    procedure IBar.Walk = BarWalk;
    procedure IFoo.Walk = FooWalk;
    procedure DoRun;
    procedure BarWalk;
    procedure FooWalk;
    procedure Stop;
  end;

procedure TX.DoRun;
begin
  WriteLn('DoRun');
end;

procedure TX.BarWalk;
begin
  WriteLn('BarWalk');
end;

procedure TX.FooWalk;
begin
  WriteLn('FooWalk');
end;

procedure TX.Stop;
begin
  WriteLn('Stop');
end;

procedure Run;
var
  F: IFoo;
  B: IBar;
begin
  B := TX.Create;
  F := TX.Create;
  B.Run;
  F.Run;
  B.Walk;
  F.Walk;
  B.Stop;
end;

begin
  Run;
end.
