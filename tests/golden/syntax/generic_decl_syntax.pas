program GenericDeclSyntax;

// The CST of generic declarations (generics.md 1.1 to 1.3, 1.5): type parameters with
// constraints separated by `,` and groups separated by `;`, nested type arguments
// (`TList<TList<Integer>>`), a `>=` closing type arguments split into `>` and `=`, generic
// method headings and implementation headings. Only the CST is checked here; the semantic
// analysis reports the undeclared `TList` as E0700 and the empty bodies of the functions as
// E2001.

type
  IHasName = interface
    function GetName: string;
  end;
  TPair<K, V> = record
    Key: K;
    Value: V;
  end;
  TOwned<K; V: class, constructor; W: IHasName> = class
  end;
  TStack<T> = class
  private
    FItems: TArray<T>;
  public
    procedure Push(const Item: T);
    function Map<U>(const Item: T): TArray<U>;
  end;
  TOuter<T> = class
  type
    TInner = class
      procedure Run;
    end;
  end;
  TNested<T>= record
    Items: TList<TList<Integer>>;
  end;

const
  Empty: TArray<Integer>= nil;

procedure TStack<T>.Push(const Item: T);
begin
end;

function TStack<T>.Map<U>(const Item: T): TArray<U>;
begin
end;

procedure TOuter<T>.TInner.Run;
begin
end;

function Max<T>(const A, B: T): T;
begin
end;

begin
end.
