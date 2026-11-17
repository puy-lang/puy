program InterfaceDispatchRules;

// Rules of interfaces without a case elsewhere (objects.md 7.1, 7.2, 7.3, 7.4, 7.8): a class that
// lists an interface again re-implements it, also for the entries of its ancestor interface; a
// child hiding a non-virtual method does not change the method the interface calls; `ToString`
// can implement an interface method; `class(IFoo)` has the parent `TObject` (a manually managed
// class, not counted); `IUnknown` is `IInterface`; interfaces can be nested in a class and
// declared in a routine; interface values in array elements and record fields; overloaded
// interface methods are chosen by their arguments; interface locals get no W0001 / W0002.

type
  IBase = interface
    procedure Run;
  end;

  IFoo = interface(IBase)
    procedure Go;
  end;

  ITo = interface
    function ToString: string;
  end;

  IOld = interface(IUnknown)
    function Name: string;
  end;

  IOver = interface
    procedure Put(X: Integer); overload;
    procedure Put(const S: string); overload;
  end;

  TA = class(TRefObject, IFoo, ITo)
    procedure Run;
    procedure Go;
  end;

  TB = class(TA)
    procedure Run;
  end;

  TC = class(TA, IFoo)
    procedure Run;
    procedure Go;
  end;

  TPlain = class(IOld)
    function Name: string;
  end;

  TOver = class(TRefObject, IOver)
    procedure Put(X: Integer); overload;
    procedure Put(const S: string); overload;
  end;

  TBox = class
  type
    INested = interface
      function Value: Integer;
    end;
  end;

  TNested = class(TRefObject, TBox.INested)
    function Value: Integer;
  end;

  TPair = record
    First: IBase;
  end;

procedure TA.Run;
begin
  WriteLn('TA.Run');
end;

procedure TA.Go;
begin
  WriteLn('TA.Go');
end;

procedure TB.Run;
begin
  WriteLn('TB.Run');
end;

procedure TC.Run;
begin
  WriteLn('TC.Run');
end;

procedure TC.Go;
begin
  WriteLn('TC.Go');
end;

function TPlain.Name: string;
begin
  Result := 'plain';
end;

procedure TOver.Put(X: Integer);
begin
  WriteLn('Put Integer ', X);
end;

procedure TOver.Put(const S: string);
begin
  WriteLn('Put string ', S);
end;

function TNested.Value: Integer;
begin
  Result := 5;
end;

procedure Local;
type
  ILocal = interface
    function Twice(X: Integer): Integer;
  end;
var
  L: ILocal;
begin
  L := nil;
  WriteLn('local: ', L = nil);
end;

procedure Quiet;
var
  F: IFoo;
begin
  F := TA.Create;
  F := nil;
end;

var
  F: IFoo;
  B: IBase;
  T: ITo;
  O: IOld;
  P: TPlain;
  V: IOver;
  N: TBox.INested;
  Arr: array of IBase;
  R: TPair;
begin
  F := TB.Create;
  F.Run;
  F := TC.Create;
  F.Run;
  F.Go;
  B := F;
  B.Run;
  T := TA.Create;
  WriteLn(T.ToString);
  P := TPlain.Create;
  O := P;
  WriteLn(O.Name, ' ', P.ClassParent.ClassName);
  P.Free;
  V := TOver.Create;
  V.Put(1);
  V.Put('one');
  N := TNested.Create;
  WriteLn('nested: ', N.Value);
  SetLength(Arr, 2);
  Arr[0] := TC.Create;
  Arr[1] := TB.Create;
  Arr[0].Run;
  Arr[1].Run;
  R.First := TC.Create;
  R.First.Run;
  Local;
  Quiet;
end.
