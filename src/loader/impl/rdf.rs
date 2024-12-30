use {
    super::super::Loader,
    crate::{
        documentor::{DocumentorImplementor, DocumentorVariant},
        model::DocumentationModel,
        source::FileSource,
        source::FileSourceImplementor,
        store::LoaderStore,
        util::{FileType, FileTypeSliceStatic, relative_path},
    },
    async_trait::async_trait,
    // futures::future::try_join_all,
    // oxigraph::store::BulkLoader,
    oxrdf::NamedNodeRef,
    oxrdfio::{RdfFormat, RdfParser},
    std::path::{Path, PathBuf},
};

/// This loader is used to load RDF files into the loader store.
/// It can load all known RDF file types.
#[derive(Debug)]
pub struct RDFLoader {}

#[async_trait]
impl Loader for RDFLoader {
    fn file_types(&self) -> FileTypeSliceStatic {
        &[
            &FileType::Turtle,
            &FileType::JSONLD,
            &FileType::RdfXml,
            &FileType::NTriples,
            &FileType::N3,
            &FileType::NQuads,
            &FileType::TriG,
        ]
    }

    /// Use the bulk loader of OxiGraph to load all the given RDF
    /// files into the given loader store.
    async fn load_files(
        &self,
        file_source: &FileSourceImplementor,
        file_names: &Vec<&PathBuf>,
        loader_store: LoaderStore,
        doc_model: DocumentationModel,
    ) -> anyhow::Result<Vec<DocumentorImplementor>> {
        let documentors = futures::future::try_join_all(
            file_names.into_iter().map(|file_name| {
                self.load_file(
                    file_source,
                    file_name.as_path(),
                    loader_store.clone(),
                    doc_model.clone(),
                )
            }),
        )
        .await?
        .into_iter()
        .flatten()
        .collect();

        Ok(documentors)
    }
}

impl std::fmt::Display for RDFLoader {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "RDF-loader")
    }
}

impl RDFLoader {
    async fn load_file(
        &self,
        file_source: &FileSourceImplementor,
        file_name: &Path,
        loader_store: LoaderStore,
        doc_model: DocumentationModel,
    ) -> anyhow::Result<Vec<DocumentorImplementor>> {
        tracing::info!(
            "Loading RDF file {:}",
            relative_path(
                file_name,
                file_source.root_path().unwrap()
            )
            .display()
        );
        let file_name_clone = file_name.to_path_buf().clone();
        let parser = self.get_parser(file_name_clone.as_path())?;
        let file_source_clone = file_source.clone();

        let documentors_result = tokio::spawn(async move {
            let bulk_loader = loader_store.store.bulk_loader();
            let file_name_x = file_name_clone.as_path();
            let reader = std::fs::File::open(file_name_x)?;
            if let Err(loader_error) =
                bulk_loader.load_from_reader(parser, reader)
            {
                tracing::error!(
                    "Error loading RDF data from {}: {}",
                    file_name_x.display(),
                    loader_error
                );
            }
            // Now check the RDF file that was just loaded by issuing
            // some SPARQL queries and see if certain
            // triples are present, for instance if we find a triple
            // like `<subject> rdf:type owl:Ontology` then we know
            // that the file that we just loaded is an OWL
            // ontology and that we therefore should create an
            // OWLOntologyDocumentor for it that will
            // further look into the just loaded RDF data.
            let mut documentors: Vec<DocumentorImplementor> = vec![];
            let documentor = DocumentorImplementor::new(
                DocumentorVariant::OWLOntology,
                Some(file_source_clone),
                Some(file_name_clone.as_path()),
                loader_store,
                doc_model,
            );
            documentors.push(documentor);
            Ok::<Vec<DocumentorImplementor>, anyhow::Error>(
                documentors,
            )
        })
        .await?;

        if let Ok(documentors) = documentors_result {
            Ok(documentors)
        } else {
            Err(anyhow::anyhow!(
                "Error loading RDF data from {}: {}",
                file_name.display(),
                documentors_result.unwrap_err()
            ))
        }
    }

    #[allow(unused)]
    fn get_parser(
        &self,
        file_name: &Path,
    ) -> anyhow::Result<RdfParser> {
        let graph_name = NamedNodeRef::new("http://example.com/g2")?;
        let base_iri = "http://example.com";

        let extension =
            file_name.extension().unwrap().to_str().unwrap();

        if let Some(format) = RdfFormat::from_extension(extension) {
            Ok(RdfParser::from_format(format)
                .with_base_iri(base_iri)?
                .without_named_graphs()
                .with_default_graph(graph_name))
        } else {
            Err(anyhow::anyhow!(
                "Unsupported file type: {}",
                extension
            ))
        }
    }
}