program AttrRuntime;

// Attribute classes written on a type, a field, a method, a property, a routine, a parameter and
// variables change nothing when the program runs (attributes.md 4.3, 7): no instance is created
// and the constructor does not run, so nothing is written by it. An attribute class is an
// ordinary reference counted class otherwise, created and freed as usual. A class name passes
// to overloaded parameters of a class reference type (`TClass`) as for any call.

uses System.SysUtils;

type
  TableAttribute = class(TCustomAttribute)
  private
    FName: string;
  public
    constructor Create(const AName: string);
    property Name: string read FName;
  end;
  KindAttribute = class(TCustomAttribute)
  public
    constructor ForClass(C: TClass);
    constructor ForNumber(N: Integer);
  end;

  [Table('users')]
  TUser = class
  public
    [Kind(TUser)] Id: Integer;
    [Kind(7)] procedure Run;
    [Table('id')] property Key: Integer read Id write Id;
  end;

constructor TableAttribute.Create(const AName: string);
begin
  WriteLn('creating ', AName);
  FName := AName;
end;

constructor KindAttribute.ForClass(C: TClass);
begin
  WriteLn('kind ', C.ClassName);
end;

constructor KindAttribute.ForNumber(N: Integer);
begin
  WriteLn('kind ', N);
end;

procedure TUser.Run;
begin
  WriteLn('run ', Id);
end;

procedure Describe(C: TClass); overload;
begin
  WriteLn('class ', C.ClassName);
end;

procedure Describe(N: Integer); overload;
begin
  WriteLn('number ', N);
end;

[Table('show')]
procedure Show([Kind(1)] const U: TUser);
var
  [Table('local')] Tag: string;
begin
  Tag := 'user';
  WriteLn(Tag, ' ', U.Key);
end;

var
  [Kind(TUser)] U: TUser;
  T: TableAttribute;
begin
  U := TUser.Create;
  U.Key := 42;
  U.Run;
  Show(U);
  U.Free;
  T := TableAttribute.Create('direct');
  WriteLn(T.Name, ' ', T.ClassName, ' ', T is TCustomAttribute);
  T := nil;
  Describe(TUser);
  Describe(3);
end.
