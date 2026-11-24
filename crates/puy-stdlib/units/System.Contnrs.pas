unit System.Contnrs;

// The containers of objects and classes that are not generic
// (docs/spec/stdlib/System.Contnrs/README.md). Written from the specification only, not from
// any other implementation.
//
// They are written on the generic containers: TObjectList is TObjectList<TObject> (so it owns
// its objects the same way), TClassList is TList<TClass>, and the stack and the queue keep their
// items in a TStack<TObject> / TQueue<TObject>. Puy has no list of pointers, so nothing here
// derives from one; the names of this unit that Puy does not provide are listed in
// System.Contnrs.unavailable (E3022).

interface

uses
  System.SysUtils, System.Generics.Collections;

type
  // A list of objects that owns them unless created with AOwnsObjects = False: all of
  // TObjectList<TObject> (the constructors, OwnsObjects, Add, Remove, Extract, Items, ...).
  TObjectList = class(TObjectList<TObject>)
  public
    // The index of the first object at AStartAt or later whose class is AClass (AExact) or
    // derives from it (not AExact); -1 if there is none. nil items are skipped. AStartAt
    // outside 0..Count raises ERangeError, as Insert does.
    function FindInstanceOf(AClass: TClass; AExact: Boolean = True;
      AStartAt: Integer = 0): Integer;
  end;

  // A list of class references: all of TList<TClass>.
  TClassList = class(TList<TClass>)
  end;

  // The base of TObjectStack and TObjectQueue: Push adds an object, Pop takes the next one off
  // and Peek looks at it, in the order the descendant keeps. Neither owns its objects.
  TOrderedList = class
  protected
    procedure PushItem(AItem: TObject); virtual; abstract;
    function PopItem: TObject; virtual; abstract;
    function PeekItem: TObject; virtual; abstract;
    function GetCount: Integer; virtual; abstract;
  public
    function Count: Integer;
    // Whether there are at least ACount objects.
    function AtLeast(ACount: Integer): Boolean;
    // Adds AItem and returns it.
    function Push(AItem: TObject): TObject;
    // Takes the next object off and returns it (EListError when empty).
    function Pop: TObject;
    // The next object, left in place (EListError when empty).
    function Peek: TObject;
  end;

  // Last in, first out.
  TObjectStack = class(TOrderedList)
  private
    FItems: TStack<TObject>;
  protected
    procedure PushItem(AItem: TObject); override;
    function PopItem: TObject; override;
    function PeekItem: TObject; override;
    function GetCount: Integer; override;
  public
    constructor Create;
    destructor Destroy; override;
  end;

  // First in, first out.
  TObjectQueue = class(TOrderedList)
  private
    FItems: TQueue<TObject>;
  protected
    procedure PushItem(AItem: TObject); override;
    function PopItem: TObject; override;
    function PeekItem: TObject; override;
    function GetCount: Integer; override;
  public
    constructor Create;
    destructor Destroy; override;
  end;

implementation

function TObjectList.FindInstanceOf(AClass: TClass; AExact: Boolean;
  AStartAt: Integer): Integer;
var
  I: Integer;
  Item: TObject;
begin
  // Same range and message as TList<T>.Insert (CheckInsertIndex, private to that unit)
  if (AStartAt < 0) or (AStartAt > Count) then
    raise ERangeError.Create('list index ' + IntToStr(AStartAt) + ' is out of range 0..' +
      IntToStr(Count));
  for I := AStartAt to Count - 1 do
  begin
    Item := Items[I];
    if Item = nil then
      Continue;
    if AExact then
    begin
      if Item.ClassType = AClass then
        Exit(I);
    end
    else if Item.InheritsFrom(AClass) then
      Exit(I);
  end;
  Result := -1;
end;

function TOrderedList.Count: Integer;
begin
  Result := GetCount;
end;

function TOrderedList.AtLeast(ACount: Integer): Boolean;
begin
  Result := GetCount >= ACount;
end;

function TOrderedList.Push(AItem: TObject): TObject;
begin
  PushItem(AItem);
  Result := AItem;
end;

function TOrderedList.Pop: TObject;
begin
  Result := PopItem;
end;

function TOrderedList.Peek: TObject;
begin
  Result := PeekItem;
end;

constructor TObjectStack.Create;
begin
  inherited Create;
  FItems := TStack<TObject>.Create;
end;

destructor TObjectStack.Destroy;
begin
  FItems.Free;
  inherited Destroy;
end;

procedure TObjectStack.PushItem(AItem: TObject);
begin
  FItems.Push(AItem);
end;

function TObjectStack.PopItem: TObject;
begin
  Result := FItems.Pop;
end;

function TObjectStack.PeekItem: TObject;
begin
  Result := FItems.Peek;
end;

function TObjectStack.GetCount: Integer;
begin
  Result := FItems.Count;
end;

constructor TObjectQueue.Create;
begin
  inherited Create;
  FItems := TQueue<TObject>.Create;
end;

destructor TObjectQueue.Destroy;
begin
  FItems.Free;
  inherited Destroy;
end;

procedure TObjectQueue.PushItem(AItem: TObject);
begin
  FItems.Enqueue(AItem);
end;

function TObjectQueue.PopItem: TObject;
begin
  Result := FItems.Dequeue;
end;

function TObjectQueue.PeekItem: TObject;
begin
  Result := FItems.Peek;
end;

function TObjectQueue.GetCount: Integer;
begin
  Result := FItems.Count;
end;

end.
