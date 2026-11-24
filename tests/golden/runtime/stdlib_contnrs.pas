program StdlibContnrs;

// What System.Contnrs does beyond the cases FPC can run (docs/spec/stdlib/System.Contnrs/README.md):
// the defaults of FindInstanceOf and its range of AStartAt, an owning TObjectList refusing a reference counted object, a
// TObjectList used as a TObjectList<TObject>, EListError from an empty TObjectStack /
// TObjectQueue, and a program's own TList (a name the unit does not provide) taking its place.

uses System.SysUtils, System.Generics.Collections, System.Contnrs;

type
  TCounted = class(TRefObject)
  end;

  // A program may declare a name System.Contnrs does not provide; it is used as declared
  TList = class
  public
    function Describe: string;
  end;

function TList.Describe: string;
begin
  Result := 'my own TList';
end;

var
  L: TObjectList;
  G: TObjectList<TObject>;
  S: TObjectStack;
  Q: TObjectQueue;
  Mine: TList;
  O: TObject;
  I: Integer;
  Starts: TArray<Integer>;
begin
  Starts := [-1, 3];
  L := TObjectList.Create;
  L.Add(TObject.Create);
  L.Add(TObject.Create);
  WriteLn('FindInstanceOf(TObject) = ', L.FindInstanceOf(TObject));
  WriteLn('FindInstanceOf(TObject, True, 1) = ', L.FindInstanceOf(TObject, True, 1));
  WriteLn('FindInstanceOf(TObject, True, 2) = ', L.FindInstanceOf(TObject, True, 2));
  for I in Starts do
    try
      L.FindInstanceOf(TObject, True, I);
    except
      on E: ERangeError do
        WriteLn(E.ClassName, ': ', E.Message);
    end;
  L.Add(nil);
  WriteLn('FindInstanceOf(TObject, False, 2) = ', L.FindInstanceOf(TObject, False, 2));
  try
    L.Add(TCounted.Create);
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn('count=', L.Count);
  G := L;
  WriteLn('as TObjectList<TObject>: count=', G.Count, ' owns=', G.OwnsObjects);
  L.Free;

  // Not owning: a reference counted object may be kept
  L := TObjectList.Create(False);
  O := TCounted.Create;
  L.Add(O);
  WriteLn('not owning: count=', L.Count);
  L.Free;

  S := TObjectStack.Create;
  try
    S.Pop;
  except
    on E: EListError do
      WriteLn('stack: ', E.ClassName);
  end;
  S.Free;
  Q := TObjectQueue.Create;
  try
    Q.Peek;
  except
    on E: EListError do
      WriteLn('queue: ', E.ClassName);
  end;
  Q.Free;

  Mine := TList.Create;
  WriteLn(Mine.Describe);
  Mine.Free;
end.
